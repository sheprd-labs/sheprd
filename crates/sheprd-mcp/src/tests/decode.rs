use crate::jsonrpc::{INVALID_PARAMS, INVALID_REQUEST, Id, Incoming, PARSE_ERROR, Request, decode};
use serde_json::json;

fn refused(line: &str) -> (Option<Id>, i64) {
    let refusal = decode(line.as_bytes()).unwrap_err();
    (refusal.id, refusal.failure.code)
}

#[test]
fn reads_requests_notifications_and_responses() {
    assert_eq!(
        decode(br#"{"jsonrpc":"2.0","id":7,"method":"tools/list","params":{"a":1}}"#).unwrap(),
        Incoming::Request(Request {
            id: Id::Number(7),
            method: "tools/list".into(),
            params: Some(json!({ "a": 1 }).as_object().unwrap().clone()),
        })
    );
    assert_eq!(
        decode(br#"{"jsonrpc":"2.0","id":"x","method":"ping"}"#).unwrap(),
        Incoming::Request(Request {
            id: Id::String("x".into()),
            method: "ping".into(),
            params: None,
        })
    );
    assert_eq!(
        decode(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap(),
        Incoming::Notification {
            method: "notifications/initialized".into(),
            params: None
        }
    );
    assert_eq!(
        decode(br#"{"jsonrpc":"2.0","id":1,"result":{}}"#).unwrap(),
        Incoming::Response
    );
}

#[test]
fn refuses_what_is_not_one_json_rpc_message() {
    assert_eq!(refused("{"), (None, PARSE_ERROR));
    assert_eq!(refused("\u{feff}{}"), (None, PARSE_ERROR));
    assert_eq!(
        refused(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#),
        (None, INVALID_REQUEST)
    );
    assert_eq!(refused("42"), (None, INVALID_REQUEST));
    assert_eq!(
        refused(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#),
        (None, INVALID_REQUEST)
    );
    assert_eq!(
        refused(r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#),
        (None, INVALID_REQUEST)
    );
    assert_eq!(
        refused(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#),
        (Some(Id::Number(1)), INVALID_REQUEST)
    );
    assert_eq!(
        refused(r#"{"jsonrpc":"2.0","id":1}"#),
        (Some(Id::Number(1)), INVALID_REQUEST)
    );
    assert_eq!(
        refused(r#"{"jsonrpc":"2.0","id":1,"method":3}"#),
        (Some(Id::Number(1)), INVALID_REQUEST)
    );
    assert_eq!(
        refused(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":[1]}"#),
        (Some(Id::Number(1)), INVALID_PARAMS)
    );
}

#[test]
fn a_string_id_longer_than_the_bound_is_refused_without_echoing_it() {
    use crate::jsonrpc::MAX_ID;
    let fits = format!(
        r#"{{"jsonrpc":"2.0","id":"{}","method":"ping"}}"#,
        "i".repeat(MAX_ID)
    );
    assert!(decode(fits.as_bytes()).is_ok());
    let long = format!(
        r#"{{"jsonrpc":"2.0","id":"{}","method":"ping"}}"#,
        "i".repeat(MAX_ID + 1)
    );
    assert_eq!(refused(&long), (None, INVALID_REQUEST));
}
