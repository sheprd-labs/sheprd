#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};
use sheprd_remote_control::control::phone::{Command, Screen};

#[test]
fn phone_requests_have_a_stable_wire_form() {
    let request = Request::Phone(Call {
        caller: Caller::default(),
        op: Ask::Drive {
            device: Some("iPad".into()),
            command: Command::Open {
                to: Screen::Chat {
                    pane: "w1-p2".into(),
                },
            },
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"phone","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"ask":"drive","device":"iPad","command":{"command":"open","to":{"screen":"chat","pane":"w1-p2"}}}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    assert_eq!(request.wait_seconds(), WAIT_SECONDS);
}

#[test]
fn phone_answers_round_trip() {
    let response = Response::Phone {
        answer: Answer::NoPhone,
    };
    let text = serde_json::to_string(&response).unwrap();
    assert_eq!(text, r#"{"status":"phone","answer":{"answer":"no_phone"}}"#);
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
}
