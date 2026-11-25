use super::*;
use crate::relay::Reply;
use serde_json::Value;

const GROUP: &str = "1234567890abcdef1234567890abcdef";

fn scoped(group: &str, slot: &str, kind: &str, byte: u8) -> String {
    seal_to(
        &relay_key(),
        &json!({
            "credential": issuer().issue(PHONE, Host::Production, NOW_S),
            "id": format!("{byte:02x}").repeat(16), "token": token_hex(byte),
            "group": group, "slot": slot, "kind": kind
        }),
        seal::HANDLE_INFO,
    )
}

fn report(revision: u64, end: bool, question: bool) -> Value {
    json!({"revision": revision, "run": 1, "end": end, "question": question,
        "working": u32::from(!end && !question), "waiting": u32::from(question),
        "done": u32::from(end), "total": 1, "state": {"sealed":"opaque"},
        "latest": end.then(|| json!({"sealed":"finished"}))})
}

async fn send(f: &Fixture, handle: &str, payload: Value) -> Reply {
    let mut push = request(handle.into(), PushType::LiveActivity);
    push.payload = payload.to_string();
    f.relay.push(source(), push, NOW_MS).await
}

fn last(f: &Fixture) -> Value {
    let sent = f.apple.sent();
    serde_json::from_slice::<Value>(&sent.last().unwrap().1.body).unwrap()["aps"].clone()
}

#[tokio::test]
async fn overlapping_hosts_flush_one_question_then_one_end_to_the_late_token() {
    let f = fixture();
    let a = "a".repeat(32);
    let b = "b".repeat(32);
    let first = scoped(GROUP, &a, "push_to_start", 1);
    let second = scoped(GROUP, &b, "push_to_start", 2);
    let (left, right) = tokio::join!(
        send(&f, &first, report(1, false, false)),
        send(&f, &second, report(1, false, true))
    );
    assert_eq!((left, right), (Reply::Delivered, Reply::Delivered));
    let start = last(&f);
    let card = start["attributes"]["shared"]["card"].as_str().unwrap();
    assert_eq!(start["event"], "start");
    assert!(!start.to_string().contains(GROUP));
    assert_eq!(
        send(&f, &first, report(2, true, false)).await,
        Reply::Delivered
    );
    assert_eq!(
        send(&f, &first, report(1, false, false)).await,
        Reply::Delivered
    );
    assert_eq!(f.apple.sent().len(), 1);
    let token = scoped(GROUP, card, "activity", 3);
    assert_eq!(send(&f, &token, json!({})).await, Reply::Delivered);
    let update = last(&f);
    let state = &update["content-state"]["shared"];
    assert_eq!(update["event"], "update");
    assert_eq!(state["slot"], b);
    assert_eq!(state["latest"]["slot"], a);
    assert_eq!(
        (state["done"].as_u64(), state["total"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(
        send(&f, &second, report(2, true, false)).await,
        Reply::Delivered
    );
    assert_eq!(last(&f)["event"], "end");
    assert_eq!(f.apple.sent().last().unwrap().0, super::token(3));
    assert_eq!(
        send(&f, &second, report(2, true, false)).await,
        Reply::Delivered
    );
    assert_eq!(send(&f, &token, json!({})).await, Reply::Gone);
    assert_eq!(f.apple.sent().len(), 3);
}

#[tokio::test]
async fn retries_reuse_the_start_nonce_and_a_finish_waits_for_the_right_generation() {
    let f = fixture();
    let slot = "a".repeat(32);
    let host = scoped(GROUP, &slot, "push_to_start", 1);
    f.apple.answer(Ok(Outcome::RateLimited));
    assert_eq!(
        send(&f, &host, report(1, false, false)).await,
        Reply::Limited
    );
    let first = last(&f);
    assert_eq!(
        send(&f, &host, report(1, false, false)).await,
        Reply::Delivered
    );
    assert_eq!(first["attributes"], last(&f)["attributes"]);
    assert_eq!(
        send(&f, &host, report(2, true, false)).await,
        Reply::Delivered
    );
    let nonce = first["attributes"]["shared"]["card"].as_str().unwrap();
    let wrong = scoped(&"f".repeat(32), nonce, "activity", 2);
    assert_eq!(send(&f, &wrong, json!({})).await, Reply::Gone);
    let right = scoped(GROUP, nonce, "activity", 3);
    assert_eq!(send(&f, &right, json!({})).await, Reply::Delivered);
    assert_eq!(last(&f)["event"], "end");
    assert_eq!(
        send(&f, &host, report(3, false, false)).await,
        Reply::Delivered
    );
    assert_ne!(first["attributes"], last(&f)["attributes"]);
    assert_eq!(send(&f, &right, json!({})).await, Reply::Gone);
}

#[tokio::test]
async fn the_phone_can_acknowledge_a_start_after_the_gateway_times_out() {
    let f = fixture();
    let host = scoped(GROUP, &"a".repeat(32), "push_to_start", 1);
    f.apple.answer(Err(sheprd_apns::Error::Timeout));
    assert_eq!(
        send(&f, &host, report(1, false, false)).await,
        Reply::Unavailable
    );
    let start = last(&f);
    let nonce = start["attributes"]["shared"]["card"].as_str().unwrap();
    let bound = scoped(GROUP, nonce, "activity", 3);
    assert_eq!(send(&f, &bound, json!({})).await, Reply::Delivered);
    assert_eq!(last(&f)["event"], "update");
    assert_eq!(
        send(&f, &host, report(1, false, false)).await,
        Reply::Delivered
    );
    assert_eq!(f.apple.sent().len(), 2);
}

#[tokio::test]
async fn a_rotated_activity_token_can_deliver_a_held_end() {
    let f = fixture();
    let host = scoped(GROUP, &"a".repeat(32), "push_to_start", 1);
    assert_eq!(
        send(&f, &host, report(1, false, false)).await,
        Reply::Delivered
    );
    let start = last(&f);
    let nonce = start["attributes"]["shared"]["card"].as_str().unwrap();
    let first = scoped(GROUP, nonce, "activity", 2);
    assert_eq!(send(&f, &first, json!({})).await, Reply::Delivered);
    f.apple.answer(Ok(Outcome::Gone));
    assert_eq!(
        send(&f, &host, report(2, true, false)).await,
        Reply::Delivered
    );
    let replacement = scoped(GROUP, nonce, "activity", 3);
    assert_eq!(send(&f, &replacement, json!({})).await, Reply::Delivered);
    assert_eq!(last(&f)["event"], "end");
    assert_eq!(f.apple.sent().last().unwrap().0, token(3));
}

#[tokio::test]
async fn replacement_keeps_a_quiet_hosts_question_and_restored_finish_counts_once() {
    let f = fixture();
    let a = "a".repeat(32);
    let asking = scoped(GROUP, &a, "push_to_start", 1);
    let working = scoped(GROUP, &"b".repeat(32), "push_to_start", 2);
    send(&f, &asking, report(1, false, true)).await;
    let original = last(&f);
    let nonce = original["attributes"]["shared"]["card"].as_str().unwrap();
    send(&f, &scoped(GROUP, nonce, "activity", 3), json!({})).await;
    f.apple.answer(Ok(Outcome::Gone));
    send(&f, &working, report(1, false, false)).await;
    let mut finish = report(2, true, false);
    finish["restored"] = json!(true);
    finish["run"] = json!(2);
    send(&f, &working, finish).await;
    let replacement = last(&f);
    assert_eq!(replacement["event"], "start");
    assert_ne!(replacement["attributes"], original["attributes"]);
    let state = &replacement["content-state"]["shared"];
    assert_eq!(state["slot"], a);
    assert_eq!(state["total"], 2);
    assert_eq!(state["done"], 1);
    assert_eq!(state["waiting"], 1);
}

#[tokio::test]
async fn a_second_question_from_one_host_alerts_even_if_the_resolution_was_paced() {
    let f = fixture();
    let host = scoped(GROUP, &"a".repeat(32), "push_to_start", 1);
    let question = |revision, question_revision| {
        let mut r = report(revision, false, true);
        r["question_revision"] = json!(question_revision);
        r
    };
    send(&f, &host, question(1, 101u64)).await;
    let start = last(&f);
    let nonce = start["attributes"]["shared"]["card"].as_str().unwrap();
    send(&f, &scoped(GROUP, nonce, "activity", 3), json!({})).await;
    assert!(last(&f)["alert"].is_null());
    send(&f, &host, question(2, 102)).await;
    assert!(last(&f)["alert"].is_object());
    send(&f, &host, question(3, 102)).await;
    assert!(last(&f)["alert"].is_null());
    // The second pane resolves while the first question is still pending.
    send(&f, &host, question(4, 101)).await;
    assert!(last(&f)["alert"].is_null());
    send(&f, &host, question(5, 103)).await;
    assert!(last(&f)["alert"].is_object());
}
