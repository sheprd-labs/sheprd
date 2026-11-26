use sheprd_chat_protocol::{MAX_PULL_REQUESTS, PullRequest, PullRequestAction};

use super::*;

fn pull_request_changes(receiver: &Receiver<ServerMessage>) -> Vec<PullRequest> {
    drain(receiver)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::Change {
                change: Change::PullRequest { pull_request },
                ..
            } => Some(pull_request),
            _ => None,
        })
        .collect()
}

fn merge_call(harness: &mut Harness, update: &str, status: &str) {
    harness.update(json!({
        "sessionUpdate": update, "toolCallId": "merge",
        "title": "`gh pr merge 7 --squash --repo o/r`", "kind": "execute",
        "status": status,
        "content": [{"type": "content", "content": {"type": "text",
            "text": "✓ Squashed and merged pull request o/r#7 (Fix)"}}],
    }));
}

#[test]
fn a_finished_gh_call_lists_its_pull_request_once() {
    let mut harness = ready();
    let receiver = harness.subscribe(1, false, None);
    drain(&receiver);
    merge_call(&mut harness, "tool_call", "in_progress");
    assert!(
        harness.transcript().pull_requests.is_empty(),
        "not before it ends"
    );
    merge_call(&mut harness, "tool_call_update", "completed");
    merge_call(&mut harness, "tool_call_update", "completed");
    let changes = pull_request_changes(&receiver);
    assert_eq!(changes.len(), 1, "a repeated update adds nothing");
    assert_eq!(changes[0].url, "https://github.com/o/r/pull/7");
    assert_eq!(changes[0].actions, [PullRequestAction::Merged]);
    assert_eq!(harness.transcript().pull_requests, changes);
}

#[test]
fn the_agent_adds_a_pull_request_by_its_address() {
    let mut harness = ready();
    let receiver = harness.subscribe(1, false, None);
    drain(&receiver);
    let add = |url: &str| Command::AddPullRequest {
        url: url.to_owned(),
    };
    harness.command(1, add(" https://github.com/o/r/pull/7/files\n"));
    assert_eq!(harness.session.handled.back().unwrap().1, None);
    harness.command(1, add("https://github.com/O/R/pull/7"));
    assert_eq!(harness.session.handled.back().unwrap().1, None);
    assert_eq!(pull_request_changes(&receiver).len(), 1, "once");
    for refused in ["https://example.com/o/r/pull/7", "o/r#7", ""] {
        harness.command(1, add(refused));
        assert_eq!(
            harness.session.handled.back().unwrap().1,
            Some(Rejection::InvalidPullRequest),
            "{refused:?}"
        );
    }
    let pulls = &harness.transcript().pull_requests;
    assert_eq!(pulls.len(), 1);
    assert!(pulls[0].actions.is_empty());
}

#[test]
fn a_full_list_refuses_a_new_pull_request() {
    let mut harness = ready();
    for number in 1..=MAX_PULL_REQUESTS {
        harness.command(
            0,
            Command::AddPullRequest {
                url: format!("https://github.com/o/r/pull/{number}"),
            },
        );
    }
    harness.command(
        0,
        Command::AddPullRequest {
            url: "https://github.com/o/r/pull/999".to_owned(),
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::TooManyPullRequests)
    );
    // What the agent does to one already listed is still recorded.
    merge_call(&mut harness, "tool_call", "completed");
    assert_eq!(
        harness.transcript().pull_requests[6].actions,
        [PullRequestAction::Merged]
    );
}

#[test]
fn switching_agents_keeps_the_listed_pull_requests() {
    let mut harness = harnesses::choosing();
    harness.command(
        1,
        Command::AddPullRequest {
            url: "https://github.com/o/r/pull/7".to_owned(),
        },
    );
    let client = harness.subscribe(2, false, None);
    drain(&client);
    harnesses::set_agent(&mut harness, Agent::Codex);
    let Some(ServerMessage::Snapshot { transcript, .. }) = drain(&client).into_iter().next() else {
        panic!("the client starts over from a snapshot");
    };
    assert_eq!(transcript.pull_requests.len(), 1);
    assert_eq!(harness.transcript().pull_requests, transcript.pull_requests);
}

#[test]
fn a_command_of_several_lines_is_read_from_the_calls_input() {
    let mut harness = ready();
    let command = "cd repo\ngit status\ngh pr merge 7 --repo o/r";
    harness.update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "merge",
        "title": crate::harness::out::command_title(command), "kind": "execute",
        "status": "in_progress", "rawInput": {"command": command},
    }));
    harness.update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "merge", "status": "completed",
    }));
    let pulls = &harness.transcript().pull_requests;
    assert_eq!(pulls.len(), 1, "the title shows only `cd repo`");
    assert_eq!(pulls[0].actions, [PullRequestAction::Merged]);
}

fn push_call(harness: &mut Harness, call: &str) {
    harness.update(json!({
        "sessionUpdate": "tool_call", "toolCallId": call,
        "title": "`git push origin feat`", "kind": "execute",
        "rawInput": {"command": "git push origin feat"},
        "status": "completed",
        "content": [{"type": "content", "content": {"type": "text",
            "text": "remote: View pull request for 'feat':\nremote:   https://github.com/o/r/pull/7"}}],
    }));
}

#[test]
fn each_turn_lists_a_pull_request_it_pushed_to_again() {
    let mut harness = ready();
    let receiver = harness.subscribe(1, false, None);
    drain(&receiver);
    let turn = prompt(&mut harness, 1, "push it");
    push_call(&mut harness, "one");
    push_call(&mut harness, "one");
    harness.respond(turn, json!({"stopReason": "end_turn"}));
    let first = pull_request_changes(&receiver);
    assert_eq!(
        first.len(),
        1,
        "a repeated update of one call lists it once"
    );
    assert_eq!(first[0].actions, [PullRequestAction::Pushed]);
    let turn = prompt(&mut harness, 1, "push again");
    push_call(&mut harness, "three");
    harness.respond(turn, json!({"stopReason": "end_turn"}));
    let again = pull_request_changes(&receiver);
    assert_eq!(again.len(), 1, "a new turn lists it again");
    assert_eq!(harness.transcript().pull_requests.len(), 1);
    assert_eq!(
        harness.transcript().pull_requests[0].actions,
        [PullRequestAction::Pushed],
        "the transcript keeps it once"
    );
    // A merge in a later turn is no push: listed only for its new action.
    prompt(&mut harness, 1, "merge");
    merge_call(&mut harness, "tool_call", "completed");
    merge_call(&mut harness, "tool_call_update", "completed");
    assert_eq!(pull_request_changes(&receiver).len(), 1);
}

fn pushed_to(harness: &mut Harness, call: &str, number: u64) {
    harness.update(json!({
        "sessionUpdate": "tool_call", "toolCallId": call,
        "title": "`git push origin feat`", "kind": "execute",
        "rawInput": {"command": "git push origin feat"},
        "status": "completed",
        "content": [{"type": "content", "content": {"type": "text",
            "text": format!("remote: View pull request for 'feat':\nremote:   https://github.com/o/r/pull/{number}")}}],
    }));
}

#[test]
fn a_later_push_in_the_turn_lists_its_pull_request_last() {
    let mut harness = ready();
    let receiver = harness.subscribe(1, false, None);
    drain(&receiver);
    prompt(&mut harness, 1, "push both");
    pushed_to(&mut harness, "a", 7);
    pushed_to(&mut harness, "b", 8);
    pushed_to(&mut harness, "c", 7);
    let numbers: Vec<u64> = pull_request_changes(&receiver)
        .iter()
        .map(|pull| pull.number)
        .collect();
    assert_eq!(numbers, [7, 8, 7], "the last one touched comes last");
}
