//! `remote test-push`: its request, its reply, and how long it waits.

use super::*;
use sheprd_remote_control::control::push::{Mode, Outcome, TestPush};

#[test]
fn test_push_has_a_stable_wire_form_and_round_trips() {
    let request: Request =
        serde_json::from_str(r#"{"method":"remote","caller":{},"op":{"op":"test_push"}}"#).unwrap();
    assert_eq!(
        request,
        Request::Remote(Call {
            caller: Caller::default(),
            op: RemoteOp::TestPush,
        })
    );
    // The daemon answers within its own wait; this outlasts it.
    assert!(
        RemoteOp::TestPush.wait_seconds()
            > sheprd_remote_control::control::RESPONSE_TIMEOUT.as_secs()
    );
    let reply = Reply::TestPush {
        mode: Mode::PushRelay,
        pushes: vec![TestPush {
            device: "iPhone".into(),
            outcome: Outcome::Delivered,
        }],
    };
    let value = serde_json::to_value(&reply).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "kind": "test_push",
            "mode": "push_relay",
            "pushes": [{"device": "iPhone", "outcome": "delivered"}],
        })
    );
    assert_eq!(serde_json::from_value::<Reply>(value).unwrap(), reply);
}
