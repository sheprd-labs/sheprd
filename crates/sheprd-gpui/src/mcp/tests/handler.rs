use super::*;
use crate::control::protocol::Via;

#[test]
fn off_it_lists_nothing_says_how_to_turn_it_on_and_runs_nothing() {
    let (mut handler, ran) = sheprd(false, done);
    assert!(handler.tools().is_empty());
    assert!(
        handler
            .instructions()
            .unwrap()
            .contains("Settings > General")
    );
    let result = handler.call(call("status", json!({}))).unwrap();
    assert!(result.is_error);
    assert!(
        text(&result).contains("MCP server is off"),
        "{}",
        text(&result)
    );
    assert!(ran.borrow().is_empty());
    // A name with no tool is still a protocol error, on or off.
    assert_eq!(handler.call(call("nope", json!({}))), Err(UnknownTool));
}

#[test]
fn on_it_lists_every_tool_and_runs_a_call_relayed_in_its_window() {
    let (mut handler, ran) = sheprd(true, done);
    assert_eq!(handler.tools().len(), tools::all().count());
    assert!(handler.instructions().unwrap().contains("memory"));
    crate::control::relay_as(None);
    let result = handler
        .call(call("pr_status", json!({ "window": 7 })))
        .unwrap();
    assert!(!result.is_error);
    assert_eq!(text(&result), "done");
    assert_eq!(structured(&result), json!({ "message": "done" }));
    assert_eq!(
        ran.borrow().as_slice(),
        [(
            ControlCommand::Pr(crate::cli::control::PrCommand::Status),
            Some(7)
        )]
    );
    assert_eq!(
        crate::control::relay(),
        Some(Via::Mcp {
            client: Some("Test Client".into())
        })
    );
    crate::control::relay_as(None);
}

#[test]
fn arguments_the_tool_refuses_never_reach_the_app() {
    let (mut handler, ran) = sheprd(true, done);
    for arguments in [
        json!({ "window": -1 }),
        json!({ "window": "front" }),
        json!({ "extra": true }),
    ] {
        let result = handler.call(call("status", arguments.clone())).unwrap();
        assert!(result.is_error, "{arguments}");
        assert!(text(&result).starts_with("refused: "), "{}", text(&result));
    }
    assert!(ran.borrow().is_empty());
}

#[test]
fn a_failure_carries_the_clis_code_and_message() {
    let unavailable = || {
        Err(crate::control::Error::Unavailable {
            path: "/tmp/sheprd.sock".into(),
            source: io::Error::from(io::ErrorKind::NotFound),
        }
        .into())
    };
    let (mut handler, _) = sheprd(true, unavailable);
    let result = handler.call(call("status", json!({}))).unwrap();
    assert!(result.is_error);
    assert!(
        text(&result).starts_with("not_running: Sheprd is not running"),
        "{}",
        text(&result)
    );
    assert_eq!(structured(&result)["error"]["code"], "not_running");
}

#[test]
fn lists_are_wrapped_and_failed_answers_are_errors() {
    let list = || Output::data("a\nb", &json!(["a", "b"]));
    let (mut handler, _) = sheprd(true, list);
    let result = handler.call(call("tab_list", json!({}))).unwrap();
    assert_eq!(structured(&result), json!({ "result": ["a", "b"] }));
    assert!(!result.is_error);
    let failed = || Ok(Output::message("1 of 2 failed").failed());
    let (mut handler, _) = sheprd(true, failed);
    assert!(handler.call(call("status", json!({}))).unwrap().is_error);
}
