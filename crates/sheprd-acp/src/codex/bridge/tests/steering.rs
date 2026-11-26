use super::*;
use crate::harness::steer;

const STEER: &str = "6f0c1a52-6a43-4e0e-9d55-2a3b8f1c0d11";

fn steer(bridge: &mut Bridge, id: i64) -> Vec<Out> {
    bridge.on_session(request(
        id,
        steer::METHOD,
        json!({"sessionId": THREAD, "id": STEER, "prompt": [{"type": "text", "text": "and the docs"}]}),
    ))
}

#[test]
fn a_steered_prompt_joins_the_running_turn() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let outs = steer(&mut bridge, 3);
    let call = harness(&outs[0]).clone();
    assert_eq!(call["method"], "turn/steer");
    assert_eq!(call["params"]["threadId"], THREAD);
    assert_eq!(call["params"]["expectedTurnId"], "turn-1");
    assert_eq!(call["params"]["clientUserMessageId"], STEER);
    assert_eq!(
        call["params"]["input"][0],
        json!({"type": "text", "text": "and the docs", "text_elements": []})
    );

    let outs = bridge.on_harness(&answer(&call["id"], json!({"turnId": "turn-1"})));
    match &outs[0] {
        Out::Session(Inbound::Notification { method, params }) => {
            assert_eq!(method, steer::STEERED);
            assert_eq!(params["id"], STEER);
        }
        other => panic!("expected the steered prompt taken, got {other:?}"),
    }
    assert!(response(&outs[1], 3).is_ok());
}

#[test]
fn a_steer_codex_refuses_is_refused_to_the_session() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let outs = steer(&mut bridge, 3);
    let id = harness(&outs[0])["id"].clone();
    let outs = bridge
        .on_harness(&json!({"id": id, "error": {"code": -32600, "message": "no active turn"}}));
    assert_eq!(outs.len(), 1);
    assert!(response(&outs[0], 3).is_err());
}

#[test]
fn a_steer_without_a_turn_is_refused() {
    let mut bridge = opened();
    let outs = steer(&mut bridge, 3);
    assert!(response(&outs[0], 3).is_err());
}
