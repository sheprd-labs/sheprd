#![allow(clippy::unwrap_used)]

use super::*;
use crate::pull_request::parse::parse;
use serde_json::{Value, json};

fn layer(
    position: u64,
    state: &str,
    merge: &str,
    review: Option<&str>,
    rollup: Option<&str>,
) -> Value {
    json!({
        "position": position,
        "pullRequest": {
            "number": 1360 + position, "title": format!("Layer {position}"),
            "state": state, "isDraft": false, "mergeStateStatus": merge, "reviewDecision": review,
            "commits": {"nodes": [{"commit": {"statusCheckRollup": rollup.map(|state| json!({"state": state}))}}]}
        }
    })
}

fn stack(entries: Vec<Value>) -> Stack {
    let size = entries.len();
    serde_json::from_value(json!({
        "number": 950, "baseRefName": "main", "size": size, "entries": {"nodes": entries}
    }))
    .unwrap()
}

#[test]
fn reads_github_stack_bottom_first_with_each_layers_rollup() {
    let stack = stack(vec![
        layer(
            2,
            "OPEN",
            "BLOCKED",
            Some("REVIEW_REQUIRED"),
            Some("PENDING"),
        ),
        layer(1, "MERGED", "UNKNOWN", None, Some("SUCCESS")),
        layer(3, "OPEN", "CLEAN", None, None),
    ]);
    assert_eq!(stack.number, 950);
    assert_eq!(stack.base, "main");
    let positions: Vec<u64> = stack.layers.iter().map(|layer| layer.position).collect();
    assert_eq!(positions, [1, 2, 3]);
    assert_eq!(stack.layers[0].checks, Some(Outcome::Passed));
    assert_eq!(stack.layers[1].checks, Some(Outcome::Pending));
    assert_eq!(stack.layers[2].checks, None);
    assert!(!stack.truncated());
    assert_eq!(stack.layers[0].head_oid, "");
}

#[test]
fn hidden_layers_and_remote_text_are_dropped_or_cleaned() {
    let mut hidden = layer(2, "OPEN", "CLEAN", None, None);
    hidden["pullRequest"] = Value::Null;
    let mut spoofed = layer(1, "OPEN", "CLEAN", None, None);
    spoofed["pullRequest"]["title"] = json!("Fix\n\u{202e}it");
    spoofed["pullRequest"]["headRefOid"] = json!("--upload-pack=evil");
    let stack: Stack = serde_json::from_value(json!({
        "number": 7, "baseRefName": "ma\u{202e}in", "size": 3,
        "entries": {"nodes": [spoofed, hidden, null]}
    }))
    .unwrap();
    assert_eq!(stack.layers.len(), 1);
    assert_eq!(stack.layers[0].title, "Fix  it");
    assert_eq!(
        stack.layers[0].head_oid, "",
        "an ID that is not a commit is dropped"
    );
    assert_eq!(stack.base, "ma in");
    assert!(stack.truncated());
}

#[test]
fn reads_at_most_the_layer_limit_the_query_asks_for() {
    let query = crate::pull_request::fetch::FIELDS;
    assert!(query.contains(&format!("entries(first: {LAYER_LIMIT})")));
    let entries = (1..=LAYER_LIMIT as u64 + 4)
        .map(|position| layer(position, "OPEN", "CLEAN", None, None))
        .collect();
    let stack = stack(entries);
    assert_eq!(stack.layers.len(), LAYER_LIMIT);
    assert!(stack.truncated());
}

#[test]
fn an_open_layer_is_unblocked_exactly_when_its_tone_is_ready() {
    for merge in [
        "CLEAN",
        "DIRTY",
        "BEHIND",
        "BLOCKED",
        "UNSTABLE",
        "DRAFT",
        "HAS_HOOKS",
        "UNKNOWN",
        "NEW",
    ] {
        for review in [
            None,
            Some("APPROVED"),
            Some("REVIEW_REQUIRED"),
            Some("CHANGES_REQUESTED"),
        ] {
            for rollup in [
                None,
                Some("SUCCESS"),
                Some("PENDING"),
                Some("FAILURE"),
                Some("ERROR"),
            ] {
                let stack = stack(vec![layer(1, "OPEN", merge, review, rollup)]);
                let layer = &stack.layers[0];
                assert_eq!(
                    layer.blocker().is_none(),
                    layer.tone() == Tone::Ready,
                    "{merge} {review:?} {rollup:?}"
                );
            }
        }
    }
}

#[test]
fn blockers_name_the_most_fundamental_reason() {
    for (merge, review, rollup, expected) in [
        (
            "DIRTY",
            Some("CHANGES_REQUESTED"),
            Some("FAILURE"),
            Blocker::Conflicts,
        ),
        (
            "BLOCKED",
            Some("CHANGES_REQUESTED"),
            Some("FAILURE"),
            Blocker::ChecksFailed,
        ),
        ("UNSTABLE", None, None, Blocker::ChecksFailed),
        (
            "BLOCKED",
            Some("CHANGES_REQUESTED"),
            Some("PENDING"),
            Blocker::ChangesRequested,
        ),
        (
            "BLOCKED",
            Some("REVIEW_REQUIRED"),
            Some("PENDING"),
            Blocker::ChecksRunning,
        ),
        (
            "BLOCKED",
            Some("REVIEW_REQUIRED"),
            Some("SUCCESS"),
            Blocker::ReviewRequired,
        ),
        ("BEHIND", None, None, Blocker::Behind),
        ("BLOCKED", None, None, Blocker::Held),
        ("HAS_HOOKS", None, None, Blocker::Undecided),
    ] {
        let stack = stack(vec![layer(1, "OPEN", merge, review, rollup)]);
        assert_eq!(
            stack.layers[0].blocker(),
            Some(expected),
            "{merge} {review:?} {rollup:?}"
        );
    }
    let mut draft = layer(1, "OPEN", "DIRTY", None, None);
    draft["pullRequest"]["isDraft"] = json!(true);
    assert_eq!(stack(vec![draft]).layers[0].blocker(), Some(Blocker::Draft));
    let closed = stack(vec![layer(1, "CLOSED", "DIRTY", None, None)]);
    assert_eq!(closed.layers[0].blocker(), Some(Blocker::Closed));
}

#[test]
fn verdict_is_the_lowest_blocked_layer_at_or_below_the_position() {
    let stack = stack(vec![
        layer(1, "MERGED", "UNKNOWN", None, Some("SUCCESS")),
        layer(2, "OPEN", "CLEAN", None, Some("SUCCESS")),
        layer(
            3,
            "OPEN",
            "BLOCKED",
            Some("REVIEW_REQUIRED"),
            Some("FAILURE"),
        ),
        layer(4, "OPEN", "DIRTY", None, None),
    ]);
    assert_eq!(stack.verdict(1), Verdict::Merged);
    assert_eq!(stack.verdict(2), Verdict::Ready { count: 1 });
    let blocked = Verdict::Blocked {
        number: 1363,
        blocker: Blocker::ChecksFailed,
    };
    assert_eq!(stack.verdict(3), blocked);
    // A conflict higher up does not hide the lower layer that blocks first.
    assert_eq!(stack.verdict(4), blocked);
    let landing: Vec<u64> = stack.landing(4).map(|layer| layer.number).collect();
    assert_eq!(landing, [1362, 1363, 1364]);
}

#[test]
fn ready_counts_every_open_layer_that_would_land() {
    let stack = stack(vec![
        layer(1, "OPEN", "CLEAN", Some("APPROVED"), Some("SUCCESS")),
        layer(2, "OPEN", "CLEAN", None, None),
        layer(3, "OPEN", "DIRTY", None, None),
    ]);
    assert_eq!(stack.verdict(2), Verdict::Ready { count: 2 });
}

#[test]
fn queue_states_github_adds_later_degrade_to_unknown() {
    let entry: QueueEntry = serde_json::from_value(
        json!({"state": "AWAITING_CHECKS", "position": 2, "estimatedTimeToMerge": 840}),
    )
    .unwrap();
    assert_eq!(entry.state, QueueState::AwaitingChecks);
    assert_eq!((entry.position, entry.eta), (2, Some(840)));
    let entry: QueueEntry = serde_json::from_value(
        json!({"state": "PAUSED", "position": 1, "estimatedTimeToMerge": null}),
    )
    .unwrap();
    assert_eq!((entry.state, entry.eta), (QueueState::Unknown, None));
}

fn pull_request(extra: Value) -> crate::pull_request::PullRequest {
    let mut pr = json!({
        "number": 1364, "url": "https://github.com/example/project/pull/1364", "title": "Sidebar",
        "state": "OPEN", "isDraft": false, "headRefName": "feature", "baseRefName": "feat/t3-client",
        "additions": 1, "deletions": 0, "changedFiles": 1, "updatedAt": "now",
        "mergeStateStatus": "BLOCKED", "headRepositoryOwner": {"login": "example"}
    });
    for (key, value) in extra.as_object().unwrap() {
        pr[key] = value.clone();
    }
    parse(&json!([pr]).to_string(), "example", "project", "feature")
        .unwrap()
        .unwrap()
}

#[test]
fn a_pull_request_reports_its_stack_and_queue_entry() {
    let pr = pull_request(json!({
        "stackEntry": {"position": 2},
        "stack": {"number": 950, "baseRefName": "main", "size": 2, "entries": {"nodes": [
            layer(1, "OPEN", "CLEAN", None, Some("SUCCESS")),
            layer(2, "OPEN", "BLOCKED", Some("REVIEW_REQUIRED"), None),
        ]}},
        "mergeQueueEntry": {"state": "QUEUED", "position": 3, "estimatedTimeToMerge": 600},
        "isMergeQueueEnabled": true
    }));
    let (stack, position) = pr.stack().unwrap();
    assert_eq!((stack.number, position), (950, 2));
    assert!(pr.merge_queue);
    assert_eq!(pr.queue.unwrap().position, 3);
}

#[test]
fn a_pull_request_outside_any_stack_or_queue_reads_as_before() {
    let pr = pull_request(json!({"stack": null, "stackEntry": null, "mergeQueueEntry": null}));
    assert!(pr.stack().is_none());
    assert!(pr.queue.is_none());
    assert!(pull_request(json!({})).stack().is_none());
}

#[test]
fn a_position_outside_the_stack_is_not_trusted() {
    let pr = pull_request(json!({
        "stackEntry": {"position": 5},
        "stack": {"number": 950, "baseRefName": "main", "size": 2, "entries": {"nodes": []}}
    }));
    assert!(pr.stack().is_none());
}

#[test]
fn only_the_lowest_open_layer_restacks_onto_a_merged_one() {
    let mut merged = layer(1, "MERGED", "UNKNOWN", None, None);
    merged["pullRequest"]["headRefOid"] = json!("0123456789abcdef0123456789abcdef01234567");
    let layers = stack(vec![
        merged,
        layer(2, "OPEN", "CLEAN", None, None),
        layer(3, "OPEN", "CLEAN", None, None),
    ]);
    assert_eq!(layers.merged_below(2).map(|layer| layer.number), Some(1361));
    assert!(
        layers.merged_below(3).is_none(),
        "layer 2 is still open between"
    );
    assert!(layers.merged_below(1).is_none());
    let unknown = stack(vec![layer(1, "MERGED", "UNKNOWN", None, None)]);
    assert!(
        unknown.merged_below(2).is_none(),
        "no head commit, nothing to name"
    );
}

#[test]
fn a_layer_left_unread_below_keeps_the_verdict_from_ready_or_merged() {
    // Layer 2 was hidden from this reader; layer 1 merged, layer 3 is ready.
    let mut hidden = layer(2, "OPEN", "DIRTY", None, None);
    hidden["pullRequest"] = Value::Null;
    let gap = stack(vec![
        layer(1, "MERGED", "UNKNOWN", None, None),
        hidden,
        layer(3, "OPEN", "CLEAN", None, None),
    ]);
    assert!(!gap.read_through(3));
    assert_eq!(gap.verdict(3), Verdict::Unread);
    assert_eq!(
        gap.verdict(1),
        Verdict::Merged,
        "below the gap is still known"
    );
    // Past the read limit, all read layers merged: not known to be merged.
    let entries = (1..=LAYER_LIMIT as u64)
        .map(|position| layer(position, "MERGED", "UNKNOWN", None, None))
        .collect();
    let mut long = stack(entries);
    long.size = LAYER_LIMIT as u64 + 1;
    assert_eq!(long.verdict(LAYER_LIMIT as u64 + 1), Verdict::Unread);
    // A blocked layer that was read still blocks.
    let blocked = stack(vec![layer(1, "OPEN", "DIRTY", None, None)]);
    assert!(matches!(blocked.verdict(3), Verdict::Blocked { .. }));
}

#[test]
fn merging_and_queueing_are_offered_only_when_every_layer_below_is_known() {
    let ready = layer(1, "OPEN", "CLEAN", Some("APPROVED"), Some("SUCCESS"));
    let full = pull_request(json!({
        "stackEntry": {"position": 2},
        "stack": {"number": 950, "baseRefName": "main", "size": 2, "entries": {"nodes": [
            ready.clone(), layer(2, "OPEN", "CLEAN", None, None),
        ]}}
    }));
    assert_eq!(full.lands(), [1361, 1362]);
    assert!(full.stack_mergeable() && full.stack_known());
    let gap = pull_request(json!({
        "stackEntry": {"position": 3},
        "stack": {"number": 950, "baseRefName": "main", "size": 3, "entries": {"nodes": [
            ready, layer(3, "OPEN", "CLEAN", None, None),
        ]}}
    }));
    assert!(!gap.stack_mergeable() && !gap.stack_known());
    let alone = pull_request(json!({}));
    assert_eq!(alone.lands(), [1364]);
    assert!(alone.stack_mergeable() && alone.stack_known());
}
