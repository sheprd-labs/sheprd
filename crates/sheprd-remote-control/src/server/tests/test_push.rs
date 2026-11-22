//! `Request::TestPush`: a test alert to each phone registered for alerts,
//! named as the user named it, and nothing for a phone that is not.

use super::*;
use crate::control::{
    Request, Response,
    push::{Mode, Outcome, TestPush, TestPushes},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

fn with_relay(dir: &std::path::Path) -> Arc<State> {
    let mut state = web::tests::state(dir);
    Arc::get_mut(&mut state).unwrap().apns =
        crate::apns::Delivery::Relay(Box::new(crate::apns::Relays::new().unwrap()));
    state
}

fn register(state: &State, device: &crate::control::DeviceId, alert: serde_json::Value) {
    let key = p256::SecretKey::from_slice(&[5; 32])
        .unwrap()
        .public_key()
        .to_sec1_bytes();
    state
        .auth()
        .devices_mut()
        .set_apns(
            device,
            Some(json!({"alert": alert, "notify_key": URL_SAFE_NO_PAD.encode(key)})),
        )
        .unwrap();
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn with_the_push_relay_off_nothing_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let runtime = runtime();
    assert_eq!(
        control::test_push::answer(&state, runtime.handle()),
        TestPushes {
            mode: Mode::Off,
            pushes: Vec::new(),
        }
    );
}

#[test]
fn each_phone_with_an_alert_handle_gets_one_and_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (relayed, _) = web::tests::signed_in(&state, 3);
    let (older_app, _) = web::tests::signed_in(&state, 4);
    // Never registered with APNs.
    let _ = web::tests::signed_in(&state, 5);
    register(
        &state,
        &relayed,
        json!({"handle": format!("1a2b3c4d.{}", "A".repeat(160)), "environment": "production"}),
    );
    // A plain token, which the push relay does not take.
    register(&state, &older_app, json!("9f".repeat(32)));
    let crate::apns::Delivery::Relay(relays) = &state.apns else {
        unreachable!()
    };
    // Out, so the push fails at once without the network.
    relays.mark_down(sheprd_apns::RelayHost::Production, web::now_ms());
    let name = state.auth().devices().get(&relayed).unwrap().name.clone();

    let runtime = runtime();
    let answer = control::test_push::answer(&state, runtime.handle());
    assert_eq!(answer.mode, Mode::PushRelay);
    assert_eq!(
        answer.pushes,
        vec![TestPush {
            device: name,
            outcome: Outcome::Unreachable,
        }]
    );
    let status = state.pushes.status(&state.apns);
    assert_eq!(status.alert.unwrap().outcome, Outcome::Unreachable);
}

#[test]
fn without_the_runtime_the_request_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let (stop, _) = watch::channel(false);
    assert!(matches!(
        control::answer(&state, &stop, Request::TestPush),
        Response::Error { .. }
    ));
}

#[test]
fn phones_without_an_alert_handle_do_not_crowd_out_one_with() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    // More plain-token phones than the cap, paired first.
    let max = u8::try_from(control::test_push::MAX_PHONES).unwrap();
    for seed in 10..10 + max + 1 {
        let (older_app, _) = web::tests::signed_in(&state, seed);
        register(&state, &older_app, json!("9f".repeat(32)));
    }
    let (relayed, _) = web::tests::signed_in(&state, 3);
    register(
        &state,
        &relayed,
        json!({"handle": format!("1a2b3c4d.{}", "A".repeat(160)), "environment": "production"}),
    );
    let crate::apns::Delivery::Relay(relays) = &state.apns else {
        unreachable!()
    };
    relays.mark_down(sheprd_apns::RelayHost::Production, web::now_ms());
    let name = state.auth().devices().get(&relayed).unwrap().name.clone();

    let runtime = runtime();
    let answer = control::test_push::answer(&state, runtime.handle());
    assert_eq!(
        answer.pushes,
        vec![TestPush {
            device: name,
            outcome: Outcome::Unreachable,
        }]
    );
}
