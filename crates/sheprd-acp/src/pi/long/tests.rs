#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;

fn long(value: &Value) -> Option<Value> {
    read(&mut serde_json::to_vec(value).unwrap().as_slice())
}

#[test]
fn a_run_keeps_how_each_message_stopped_and_drops_its_pictures() {
    let picture = "A".repeat(64 * 1024);
    let line = json!({"type": "agent_end", "messages": [
        {"role": "user", "content": [{"type": "text", "text": "Record it"}]},
        {"role": "toolResult", "toolCallId": "t1", "content": [{"type": "image", "data": picture, "mimeType": "image/png"}]},
        {"role": "assistant", "content": [{"type": "text", "text": "Done"}], "stopReason": "stop",
            "usage": {"input": 2, "output": 1036}, "api": "anthropic-messages"},
    ]});
    assert_eq!(
        long(&line).unwrap(),
        json!({"type": "agent_end", "messages": [
            {"role": "user"},
            {"role": "toolResult"},
            {"role": "assistant", "stopReason": "stop", "usage": {"input": 2, "output": 1036}},
        ]})
    );
}

#[test]
fn a_turn_drops_its_tool_results_and_keeps_everything_else() {
    let line = json!({"type": "turn_end", "id": 7, "extra": {"kept": [1, 2.5, true, null]},
        "message": {"role": "assistant", "stopReason": "toolUse", "content": [{"type": "text", "text": "x"}]},
        "toolResults": [{"role": "toolResult", "content": [{"type": "image", "data": "AAAA"}]}]});
    assert_eq!(
        long(&line).unwrap(),
        json!({"type": "turn_end", "id": 7, "extra": {"kept": [1, 2.5, true, null]},
            "message": {"role": "assistant", "stopReason": "toolUse"}})
    );
}

#[test]
fn a_message_that_is_not_one_of_pis_is_kept_as_it_came() {
    let line = json!({"type": "extension_ui_request", "id": "q", "method": "notify", "message": "Saved", "messages": null});
    assert_eq!(
        long(&line).unwrap(),
        json!({"type": "extension_ui_request", "id": "q", "method": "notify", "message": "Saved"})
    );
}

#[test]
fn a_line_that_is_not_json_is_skipped() {
    assert!(read(&mut b"npm warn deprecated".as_slice()).is_none());
    assert!(read(&mut br#"{"type":"agent_end","messages":[{"role":"#.as_slice()).is_none());
}
