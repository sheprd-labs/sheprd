#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::tabs::TabOp;

#[test]
fn calls_round_trip_and_refuse_unknown_fields() {
    let call = LendCall {
        borrower: "studio".to_owned(),
        owner: "pane:w1-2".to_owned(),
        request: LendRequest::Frame {
            name: "default".to_owned(),
            width: 1280,
            since: Some("abc".to_owned()),
        },
    };
    let text = serde_json::to_string(&call).unwrap();
    assert_eq!(serde_json::from_str::<LendCall>(&text).unwrap(), call);
    let tab = LendRequest::Tab {
        op: TabOp::MachineStart {
            name: "default".to_owned(),
        },
    };
    let text = serde_json::to_string(&tab).unwrap();
    assert_eq!(serde_json::from_str::<LendRequest>(&text).unwrap(), tab);
    let extra = r#"{"borrower":"a","owner":"b","request":{"kind":"picture","name":"x"},"more":1}"#;
    assert!(serde_json::from_str::<LendCall>(extra).is_err());
}

#[test]
fn only_machine_work_is_lendable() {
    let name = || "default".to_owned();
    assert!(TabOp::MachineStart { name: name() }.lendable());
    assert!(TabOp::MachineList.lendable());
    assert!(!TabOp::Machine { name: name() }.lendable());
    assert!(TabOp::Machine { name: name() }.forwardable());
    assert!(!TabOp::MachineDelete { name: name() }.forwardable());
    assert!(!TabOp::Devices.forwardable());
    let nested = TabOp::MachineOn {
        host: "mini".to_owned(),
        request: Box::new(TabOp::MachineList),
    };
    assert!(!nested.forwardable());
    let peek = TabOp::MachinePeek {
        name: name(),
        path: "/tmp/x.png".to_owned(),
    };
    assert!(peek.lendable());
    assert!(!peek.forwardable());
}
