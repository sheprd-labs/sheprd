//! The optional `id` on a listener's messages: read when present, absent
//! from what an old client sends and what the relay answers it, and bounded.

use crate::socket::{FromListener, Id, MAX_ID, ToPeer};
use crate::wire::Status;

#[test]
fn an_old_client_sends_and_hears_no_id() {
    let keys: FromListener = serde_json::from_str(r#"{"t":"dial_keys","keys":[]}"#).unwrap();
    assert_eq!(keys.id(), None);
    let status = ToPeer::Status {
        id: None,
        status: Status::Ok,
    };
    assert_eq!(
        serde_json::to_string(&status).unwrap(),
        r#"{"t":"status","status":"ok"}"#
    );
}

#[test]
fn numbers_and_text_are_read_and_echoed_as_given() {
    let keys: FromListener =
        serde_json::from_str(r#"{"t":"dial_keys","keys":[],"id":42}"#).unwrap();
    assert_eq!(keys.id(), Some(&Id::Number(42)));
    let pairing: FromListener =
        serde_json::from_str(r#"{"t":"pairing_key","key":"AAAA","seconds":5,"id":"pair-1"}"#)
            .unwrap();
    assert_eq!(pairing.id(), Some(&Id::Text("pair-1".into())));
    let status = ToPeer::Status {
        id: Some(Id::Text("pair-1".into())),
        status: Status::Ok,
    };
    assert_eq!(
        serde_json::to_string(&status).unwrap(),
        r#"{"t":"status","id":"pair-1","status":"ok"}"#
    );
}

#[test]
fn a_long_text_id_makes_the_message_malformed() {
    let long = FromListener::DialKeys {
        id: Some(Id::Text("x".repeat(MAX_ID + 1))),
        keys: Vec::new(),
    };
    assert_eq!(long.decode(), None);
    let fits = FromListener::DialKeys {
        id: Some(Id::Text("x".repeat(MAX_ID))),
        keys: Vec::new(),
    };
    assert!(fits.decode().is_some());
}
