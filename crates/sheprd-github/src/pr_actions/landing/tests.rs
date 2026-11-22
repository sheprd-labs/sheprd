#![allow(clippy::unwrap_used)]

use super::*;
use crate::pr_actions::{Action, perform};
use crate::pull_request::fixture;

const UUID: &str = "3f2b6c1e-8a4d-4f0b-9c2e-5d7a1b0e9f12";
const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
const PATH: &str = "/repos/example/project/pulls/8/merge-async";

fn target() -> Target {
    Target::try_from(&fixture().unwrap()).unwrap()
}

fn pending() -> Value {
    json!({"status": "pending", "details": {
        "message": "Merge request accepted", "uuid": UUID, "merge_method": "squash",
        "merge_action": "direct_merge", "expected_head_sha": HEAD
    }})
}

#[test]
fn merging_a_stack_names_the_seen_head_and_reads_a_background_merge_until_it_lands() {
    let mut steps = Vec::new();
    let mut reads = 0;
    let outcome = land(&target(), Landing::Stack(MergeMethod::Squash), |step| {
        steps.push(step.clone());
        Ok(match step {
            Step::Put { .. } => pending(),
            Step::Get { .. } => {
                reads += 1;
                if reads < 2 {
                    pending()
                } else {
                    json!({"status": "merged", "details": {"message": "Merged", "sha": HEAD}})
                }
            }
            Step::Pause(_) => Value::Null,
        })
    })
    .unwrap();
    assert_eq!(
        outcome.message,
        "Merged #8 and the open pull requests below it"
    );
    let read = Step::Get {
        path: format!("{PATH}/{UUID}"),
    };
    assert_eq!(
        steps,
        [
            Step::Put {
                path: PATH.into(),
                body: json!({"sha": HEAD, "merge_action": "direct_merge", "merge_method": "squash"}),
            },
            Step::Pause(POLL_EVERY),
            read.clone(),
            Step::Pause(POLL_EVERY),
            read,
        ]
    );
}

#[test]
fn queueing_sends_the_queue_action_without_a_merge_method() {
    let mut puts = Vec::new();
    let outcome = land(&target(), Landing::Queue, |step| {
        if let Step::Put { body, .. } = &step {
            puts.push(body.clone());
        }
        Ok(json!({"status": "enqueued", "details": {"message": "Added"}}))
    })
    .unwrap();
    assert_eq!(puts, [json!({"sha": HEAD, "merge_action": "merge_queue"})]);
    assert_eq!(outcome.message, "Added #8 to the merge queue");
}

#[test]
fn a_refusal_is_reported_once_and_never_sent_again() {
    let mut sends = 0;
    let result = land(&target(), Landing::Stack(MergeMethod::Merge), |_| {
        sends += 1;
        Err(crate::Error::Rejected(
            "Pull request is not mergeable".into(),
        ))
    });
    assert!(
        matches!(result, Err(crate::Error::Rejected(reason)) if reason == "Pull request is not mergeable")
    );
    assert_eq!(sends, 1);
}

#[test]
fn a_failed_merge_says_why_in_cleaned_text() {
    let answer = json!({"status": "failed", "details": {"message": "Base\u{202e} branch moved\n"}});
    assert!(matches!(
        outcome(&answer, &target(), Landing::Queue),
        Err(crate::Error::Rejected(reason)) if reason == "Base  branch moved"
    ));
}

#[test]
fn a_merge_still_running_after_the_last_read_is_a_success_that_says_so() {
    let mut reads = 0;
    let outcome = land(&target(), Landing::Stack(MergeMethod::Rebase), |step| {
        if matches!(step, Step::Get { .. }) {
            reads += 1;
        }
        Ok(pending())
    })
    .unwrap();
    assert_eq!(reads, POLLS);
    assert!(outcome.message.contains("still landing"));
}

#[test]
fn a_request_id_that_is_not_a_uuid_is_never_put_in_a_path() {
    let mut steps = 0;
    let outcome = land(&target(), Landing::Queue, |_| {
        steps += 1;
        let mut answer = pending();
        answer["details"]["uuid"] = json!("../../../user");
        Ok(answer)
    })
    .unwrap();
    assert_eq!(steps, 1);
    assert!(outcome.message.contains("still landing"));
    assert!(matches!(
        outcome_of(json!({"status": "surprise"})),
        Err(crate::Error::PullRequest(pull_request::Error::Identity))
    ));
}

fn outcome_of(answer: Value) -> crate::Result<Outcome> {
    outcome(&answer, &target(), Landing::Queue)
}

#[test]
fn dequeueing_sends_one_mutation_for_the_pull_request() {
    let mut sent = Vec::new();
    let outcome = perform(
        &target(),
        Action::Dequeue,
        |context, query, variables| {
            sent.push((context, query.to_owned(), variables));
            Ok(json!({"data": {"dequeuePullRequest": {"mergeQueueEntry": null}}}))
        },
        |_| unreachable!("dequeueing is a mutation"),
    )
    .unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "pr_dequeue");
    assert!(sent[0].1.contains("dequeuePullRequest"));
    assert_eq!(sent[0].2, json!({"id": "PR_kwDOfixture8"}));
    assert_eq!(outcome.message, "Removed #8 from the merge queue");
}

#[test]
fn landing_through_perform_uses_the_rest_steps_not_a_mutation() {
    let outcome = perform(
        &target(),
        Action::Land(Landing::Queue),
        |_, _, _| unreachable!("landing is not a mutation"),
        |_| Ok(json!({"status": "enqueued", "details": {"message": "Added"}})),
    )
    .unwrap();
    assert_eq!(outcome.url.as_deref(), Some(target().url.as_str()));
}

#[test]
fn merging_a_stacked_pull_request_lands_it_with_the_layers_below() {
    let stacked = Target::try_from(&pull_request::ready_stacked_fixture().unwrap()).unwrap();
    let mut puts = Vec::new();
    let outcome = perform(
        &stacked,
        Action::Merge(MergeMethod::Rebase),
        |_, _, _| unreachable!("a stacked merge is not the synchronous mutation"),
        |step| {
            if let Step::Put { path, body } = step {
                puts.push((path, body));
            }
            Ok(json!({"status": "merged", "details": {"message": "Merged", "sha": HEAD}}))
        },
    )
    .unwrap();
    assert_eq!(puts.len(), 1);
    assert_eq!(puts[0].0, "/repos/example/project/pulls/1364/merge-async");
    assert_eq!(puts[0].1["merge_method"], "rebase");
    assert!(outcome.message.contains("below it"));
}

#[test]
fn an_accepted_landing_whose_result_cannot_be_read_is_not_called_a_failure() {
    let mut reads = 0;
    let outcome = land(
        &target(),
        Landing::Stack(MergeMethod::Squash),
        |step| match step {
            Step::Put { .. } => Ok(pending()),
            Step::Pause(_) => Ok(Value::Null),
            Step::Get { .. } => {
                reads += 1;
                Err(crate::Error::Status(502))
            }
        },
    )
    .unwrap();
    assert_eq!(reads, 1, "a failed read is not retried");
    assert!(outcome.message.contains("accepted"), "{}", outcome.message);
    assert_eq!(outcome.url.as_deref(), Some(target().url.as_str()));
}
