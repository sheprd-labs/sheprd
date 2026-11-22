use super::*;
use crate::protocol::settings::SettingsOp;

fn mcp(client: Option<&str>) -> Via {
    Via::Mcp {
        client: client.map(str::to_owned),
    }
}

#[test]
fn a_relayed_caller_has_a_stable_wire_form_and_a_direct_one_omits_it() {
    let caller = Caller {
        via: Some(mcp(Some("Cursor"))),
        ..Caller::default()
    };
    let text = serde_json::to_string(&caller).unwrap();
    assert_eq!(
        text,
        r#"{"pane_id":null,"workspace_id":null,"daemon_socket":null,"via":{"mcp":{"client":"Cursor"}}}"#
    );
    assert_eq!(serde_json::from_str::<Caller>(&text).unwrap(), caller);
    let direct = serde_json::to_string(&Caller::default()).unwrap();
    assert!(!direct.contains("via"), "{direct}");
    let nameless: Caller = serde_json::from_str(r#"{"via":{"mcp":{}}}"#).unwrap();
    assert_eq!(nameless.via, Some(mcp(None)));
}

#[test]
fn unknown_relays_and_fields_are_refused() {
    for invalid in [
        r#"{"via":{"ssh":{}}}"#,
        r#"{"via":{"mcp":{"client":"a","pid":1}}}"#,
        r#"{"via":"mcp"}"#,
    ] {
        assert!(
            serde_json::from_str::<Caller>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn every_request_reports_its_relay() {
    let via = Some(mcp(Some("Codex")));
    let caller = Caller {
        via: via.clone(),
        ..Caller::default()
    };
    let settings = Request::Settings(Call {
        caller: caller.clone(),
        op: SettingsOp::Update,
    });
    assert_eq!(settings.via(), via.as_ref());
    assert_eq!(Request::Status(caller).via(), via.as_ref());
    assert_eq!(Request::Status(Caller::default()).via(), None);
    let feedback = Request::Feedback(FeedbackRequest {
        pane_id: "p1".into(),
        daemon_socket: None,
        wait_seconds: 0,
        via: via.clone(),
    });
    assert_eq!(feedback.via(), via.as_ref());
}

#[test]
fn set_via_marks_and_clears_any_request() {
    let via = mcp(Some("Zed"));
    let mut status = Request::Status(Caller::default());
    status.set_via(Some(via.clone()));
    assert_eq!(status.via(), Some(&via));
    status.set_via(None);
    assert_eq!(status, Request::Status(Caller::default()));
    let mut feedback = Request::Feedback(FeedbackRequest {
        pane_id: "p1".into(),
        daemon_socket: None,
        wait_seconds: 0,
        via: None,
    });
    feedback.set_via(Some(via.clone()));
    assert_eq!(feedback.via(), Some(&via));
}

#[test]
fn client_names_must_be_printable_and_bounded() {
    assert!(mcp(None).is_valid());
    assert!(mcp(Some("Claude Desktop")).is_valid());
    assert!(mcp(Some(&"x".repeat(MAX_CLIENT_NAME))).is_valid());
    assert!(!mcp(Some(&"x".repeat(MAX_CLIENT_NAME + 1))).is_valid());
    assert!(!mcp(Some("evil\u{1b}[2J")).is_valid());
}
