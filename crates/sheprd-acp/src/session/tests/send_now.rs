use super::*;
use crate::harness::steer;

fn typed(harness: &mut Harness, text: &str) -> CommandId {
    harness.command(
        0,
        Command::Prompt {
            origin: None,
            text: text.to_owned(),
            attachments: Vec::new(),
        },
    )
}

fn prompts(harness: &Harness) -> Vec<Value> {
    harness
        .sent()
        .into_iter()
        .filter(|message| message["method"] == "session/prompt")
        .collect()
}

fn texts(harness: &Harness) -> Vec<(String, bool)> {
    harness
        .transcript()
        .queued
        .iter()
        .map(|queued| (queued.text.clone(), queued.held))
        .collect()
}

#[test]
fn a_stopped_turns_queue_waits_until_sent_now() {
    let mut harness = ready();
    let first = prompt(&mut harness, 0, "one");
    typed(&mut harness, "two");
    let three = typed(&mut harness, "three");
    harness.command(0, Command::Cancel);
    harness.respond(first, json!({"stopReason": "cancelled"}));
    assert!(prompts(&harness).is_empty(), "nothing sent after the stop");
    assert_eq!(
        texts(&harness),
        [("two".to_owned(), true), ("three".to_owned(), true)]
    );

    // A new prompt goes at once, and the held ones still wait after it.
    let four = prompt(&mut harness, 0, "four");
    harness.respond(four, json!({"stopReason": "end_turn"}));
    assert!(prompts(&harness).is_empty());

    harness.command(0, Command::SendNow { id: three });
    let sent = prompts(&harness);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["params"]["prompt"][0]["text"], "three");
    assert_eq!(texts(&harness), [("two".to_owned(), true)]);
}

#[test]
fn send_now_stops_a_turn_the_agent_cannot_steer_and_goes_first() {
    let mut harness = ready();
    let first = prompt(&mut harness, 0, "one");
    typed(&mut harness, "two");
    let three = typed(&mut harness, "three");
    harness.command(0, Command::SendNow { id: three });
    harness.last_request("session/cancel");
    assert_eq!(
        texts(&harness),
        [("three".to_owned(), false), ("two".to_owned(), false)]
    );
    harness.respond(first, json!({"stopReason": "cancelled"}));
    let sent = prompts(&harness);
    assert_eq!(sent[0]["params"]["prompt"][0]["text"], "three");
    assert_eq!(texts(&harness), [("two".to_owned(), false)]);
}

#[test]
fn send_now_steers_a_held_prompt_into_a_turn_as_first_sent() {
    let mut harness = harness_with(config());
    harness.session.start();
    let initialize = harness.last_request("initialize");
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {"_meta": {steer::CAPABILITY: true}}}),
    );
    let new = harness.last_request("session/new");
    harness.respond(new["id"].as_i64().unwrap(), json!({"sessionId": "s1"}));

    let first = prompt(&mut harness, 0, "one");
    let two = typed(&mut harness, "two");
    harness.command(0, Command::SendNow { id: two });
    let steered = harness.last_request(steer::METHOD);
    harness.command(0, Command::Cancel);
    harness.respond(first, json!({"stopReason": "cancelled"}));
    assert_eq!(texts(&harness), [("two".to_owned(), true)]);

    prompt(&mut harness, 0, "three");
    harness.command(0, Command::SendNow { id: two });
    let again = harness.last_request(steer::METHOD);
    assert_eq!(again["params"]["prompt"], steered["params"]["prompt"]);
    let queued = &harness.transcript().queued;
    assert!(queued[0].steered && !queued[0].held);
}

#[test]
fn send_now_refuses_a_prompt_no_longer_queued() {
    let mut harness = ready();
    harness.command(
        0,
        Command::SendNow {
            id: CommandId::random(),
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownQueued)
    );
}
