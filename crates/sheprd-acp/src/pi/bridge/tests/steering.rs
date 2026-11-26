use std::time::Instant;

use super::*;
use crate::harness::steer;

const STEER: &str = "6f0c1a52-6a43-4e0e-9d55-2a3b8f1c0d11";
const TEXT: &str = "and the docs";

/// Steers `TEXT` and has pi queue it, as pi 1.5 does: the queue lists it
/// before pi answers.
fn steered(bridge: &mut Bridge, id: i64) -> Vec<Out> {
    let outs = bridge.on_session(request(
        id,
        steer::METHOD,
        json!({"sessionId": SESSION, "id": STEER, "prompt": [{"type": "text", "text": TEXT}]}),
    ));
    let command = harness(&outs[0]).clone();
    assert_eq!(command["type"], "steer");
    assert_eq!(command["message"], TEXT);
    assert!(queue(bridge, &[TEXT]).is_empty());
    bridge.on_harness(&done(&command, json!({"disposition": "queued"})))
}

fn queue(bridge: &mut Bridge, steering: &[&str]) -> Vec<Out> {
    bridge.on_harness(&json!({"type": "queue_update", "steering": steering, "followUp": []}))
}

fn notified<'a>(outs: &'a [Out], method: &str) -> Option<&'a Value> {
    outs.iter().find_map(|out| match out {
        Out::Session(Inbound::Notification {
            method: sent,
            params,
        }) if sent == method => Some(&params["id"]),
        _ => None,
    })
}

fn answered(outs: &[Out], id: i64) -> bool {
    outs.iter().any(|out| {
        matches!(out, Out::Session(Inbound::Response { id: wire::RequestId::Number(answered), .. }) if *answered == id)
    })
}

fn end(bridge: &mut Bridge) -> Vec<Out> {
    bridge.on_harness(&json!({"type": "agent_end", "messages": [
        {"role": "assistant", "stopReason": "stop", "usage": {"input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0}},
    ]}))
}

#[test]
fn a_steered_prompt_is_taken_when_pis_queue_lets_it_go() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let outs = steered(&mut bridge, 3);
    assert!(response(&outs[0], 3).is_ok());
    assert!(notified(&outs, steer::STEERED).is_none(), "still queued");
    let outs = queue(&mut bridge, &[]);
    assert_eq!(notified(&outs, steer::STEERED), Some(&json!(STEER)));
    assert!(answered(&end(&mut bridge), 2));
}

#[test]
fn a_run_ending_with_a_queued_steer_waits_for_the_run_that_takes_it() {
    let mut bridge = opened();
    // This pi says when it settles.
    bridge.on_harness(&json!({"type": "agent_settled", "aborted": false}));
    prompt(&mut bridge, 2);
    steered(&mut bridge, 3);
    assert!(!answered(&end(&mut bridge), 2), "pi goes on with it");
    let outs = queue(&mut bridge, &[]);
    assert_eq!(notified(&outs, steer::STEERED), Some(&json!(STEER)));
    assert!(answered(&end(&mut bridge), 2));
}

#[test]
fn a_steer_pi_kept_once_it_settled_is_given_back() {
    let mut bridge = opened();
    bridge.on_harness(&json!({"type": "agent_settled", "aborted": false}));
    prompt(&mut bridge, 2);
    steered(&mut bridge, 3);
    end(&mut bridge);
    let outs = bridge.on_harness(&json!({"type": "agent_settled", "aborted": false}));
    assert!(
        outs.iter()
            .any(|out| matches!(out, Out::Harness(command) if command["type"] == "clear_queue"))
    );
    assert_eq!(notified(&outs, steer::UNSTEERED), Some(&json!(STEER)));
    assert!(answered(&outs, 2));
}

#[test]
fn a_first_run_ending_with_a_queued_steer_waits_for_pi_to_continue() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    steered(&mut bridge, 3);
    // pi has not been heard to settle yet, so it gets a while to continue.
    let outs = end(&mut bridge);
    assert!(!answered(&outs, 2));
    assert!(notified(&outs, steer::UNSTEERED).is_none(), "{outs:?}");
    assert!(bridge.next_tick().is_some());
    // It continued: the prompt waits for that run however long it takes.
    bridge.on_harness(&json!({"type": "agent_start"}));
    assert!(bridge.held_until.is_none());
    let outs = queue(&mut bridge, &[]);
    assert_eq!(notified(&outs, steer::STEERED), Some(&json!(STEER)));
    let outs = end(&mut bridge);
    assert!(answered(&outs, 2));
    assert!(notified(&outs, steer::UNSTEERED).is_none());
}

#[test]
fn a_pi_that_neither_continues_nor_settles_gives_queued_steers_back() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    steered(&mut bridge, 3);
    assert!(!answered(&end(&mut bridge), 2));
    bridge.held_until = Some(Instant::now());
    let outs = bridge.tick();
    assert!(
        outs.iter()
            .any(|out| matches!(out, Out::Harness(command) if command["type"] == "clear_queue"))
    );
    assert_eq!(notified(&outs, steer::UNSTEERED), Some(&json!(STEER)));
    assert!(answered(&outs, 2));
    assert!(bridge.next_tick().is_none());
}

#[test]
fn a_steer_without_a_turn_is_refused() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(
        3,
        steer::METHOD,
        json!({"sessionId": SESSION, "id": STEER, "prompt": [{"type": "text", "text": TEXT}]}),
    ));
    assert!(response(&outs[0], 3).is_err());
}
