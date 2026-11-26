use super::*;
use crate::harness::steer as steering;

const STEER: &str = "6f0c1a52-6a43-4e0e-9d55-2a3b8f1c0d11";

fn steer(bridge: &mut Bridge, id: i64, text: &str) -> Vec<Out> {
    bridge.on_session(request(
        id,
        steering::METHOD,
        json!({"sessionId": SESSION, "id": STEER, "prompt": [{"type": "text", "text": text}]}),
    ))
}

/// Claude's echo of a user message it took, as `--replay-user-messages`
/// sends it.
fn echo(uuid: &str, text: &str) -> Value {
    json!({
        "type": "user",
        "uuid": uuid,
        "isReplay": true,
        "message": {"role": "user", "content": [{"type": "text", "text": text}]},
        "parent_tool_use_id": null,
        "session_id": SESSION,
    })
}

/// The steering notification an out sends the session.
fn notified(out: &Out) -> (&str, &Value) {
    match out {
        Out::Session(Inbound::Notification { method, params }) => {
            assert_eq!(params["sessionId"], SESSION);
            (method.as_str(), &params["id"])
        }
        other => panic!("expected a notification, got {other:?}"),
    }
}

#[test]
fn a_steered_prompt_goes_to_claude_by_its_uuid_and_is_taken_when_echoed() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "run the tests");
    let outs = steer(&mut bridge, 3, "skip the slow ones");
    let message = claude(&outs[0]);
    assert_eq!(message["type"], "user");
    assert_eq!(message["uuid"], STEER);
    assert_eq!(
        message["message"]["content"][0]["text"],
        "skip the slow ones"
    );
    assert!(response(&outs[1], 3).is_ok());

    // The echo of the prompt that started the turn says nothing.
    assert!(
        bridge
            .on_harness(&echo(
                "11111111-1111-4111-8111-111111111111",
                "run the tests"
            ))
            .is_empty()
    );
    let outs = bridge.on_harness(&echo(STEER, "skip the slow ones"));
    assert_eq!(notified(&outs[0]), (steering::STEERED, &json!(STEER)));

    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    assert!(response(outs.last().unwrap(), 2).is_ok());
}

#[test]
fn a_turn_that_ends_before_claude_takes_the_steer_waits_for_the_next() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "write a poem");
    steer(&mut bridge, 3, "say banana");
    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    assert!(
        outs.iter()
            .all(|out| !matches!(out, Out::Session(Inbound::Response { .. }))),
        "the prompt goes on: {outs:?}"
    );
    bridge.on_harness(&echo(STEER, "say banana"));
    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    let answer = response(outs.last().unwrap(), 2).as_ref().unwrap();
    assert_eq!(answer["stopReason"], "end_turn");
}

#[test]
fn cancelling_withdraws_steered_prompts_before_interrupting() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "run the tests");
    steer(&mut bridge, 3, "and lint");
    let outs = bridge.on_session(Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({"sessionId": SESSION}),
    });
    let withdraw = claude(&outs[0]);
    assert_eq!(withdraw["request"]["subtype"], "cancel_async_message");
    assert_eq!(withdraw["request"]["message_uuid"], STEER);
    assert_eq!(claude(&outs[1])["request"]["subtype"], "interrupt");

    // Cancelled, the turn ends with its result whatever Claude kept.
    let outs = bridge.on_harness(&result("error_during_execution", None));
    let answer = response(outs.last().unwrap(), 2).as_ref().unwrap();
    assert_eq!(answer["stopReason"], "cancelled");
}

#[test]
fn a_steer_without_a_running_turn_is_refused() {
    let mut bridge = opened();
    let outs = steer(&mut bridge, 3, "hello");
    assert!(response(&outs[0], 3).is_err());
}
