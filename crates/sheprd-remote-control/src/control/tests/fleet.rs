use super::*;
use crate::control::fleet::{
    Method,
    sync::{Answer, Apply, Ask},
};

#[test]
fn fleet_requests_keep_their_wire_names() {
    assert_eq!(
        serde_json::to_value(Request::Fleet(Box::new(Ask::Status))).unwrap(),
        json!({"request": "fleet", "ask": "status"})
    );
    assert_eq!(
        serde_json::to_value(Request::Fleet(Box::new(Ask::Admit {
            bundle: "b".into(),
            signature: "s".into(),
            method: Method::Bluetooth,
        })))
        .unwrap(),
        json!({"request": "fleet", "ask": "admit", "bundle": "b", "signature": "s", "method": "bluetooth"})
    );
    let apply: Request = serde_json::from_value(json!({
        "request": "fleet", "ask": "apply", "heads": ["h"], "ops": []
    }))
    .unwrap();
    assert_eq!(
        apply,
        Request::Fleet(Box::new(Ask::Apply(Apply {
            fleet: None,
            heads: vec!["h".into()],
            ops: Vec::new(),
            nonce: None,
            peer: None,
            rejoin: false,
        })))
    );
    assert_eq!(
        serde_json::to_value(Response::Fleet(Box::new(Answer::Done))).unwrap(),
        json!({"response": "fleet", "answer": "done"})
    );
    assert_eq!(
        serde_json::to_value(Response::Fleet(Box::new(Answer::Status {
            view: None,
            me: None,
        })))
        .unwrap(),
        json!({"response": "fleet", "answer": "status"})
    );
    assert_eq!(
        serde_json::to_value(Response::Fleet(Box::new(Answer::Status {
            view: None,
            me: Some("ab".repeat(32)),
        })))
        .unwrap(),
        json!({"response": "fleet", "answer": "status", "me": "ab".repeat(32)})
    );
}

#[test]
fn joins_keep_their_wire_names() {
    use crate::control::fleet::{Kind, sync::Join};
    assert_eq!(
        serde_json::to_value(Request::Fleet(Box::new(Ask::Joins))).unwrap(),
        json!({"request": "fleet", "ask": "joins"})
    );
    let answer = Response::Fleet(Box::new(Answer::Joins {
        joins: vec![Join {
            member: "m".into(),
            name: "phone".into(),
            kind: Kind::Phone,
            bundle: "b".into(),
            signature: "s".into(),
            device: "iPhone".into(),
            device_id: DeviceId("d1".into()),
            received_ms: 5,
        }],
    }));
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({"response": "fleet", "answer": "joins", "joins": [{
            "member": "m", "name": "phone", "kind": "phone", "bundle": "b",
            "signature": "s", "device": "iPhone", "device_id": "d1",
            "received_ms": 5,
        }]})
    );
}
