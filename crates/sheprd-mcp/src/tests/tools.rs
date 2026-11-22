use super::*;
use crate::ClientInfo;

#[test]
fn lists_the_handlers_tools() {
    let server = Running::start();
    server.initialize();
    let answer = server.ask(1, "tools/list", json!({}));
    assert_eq!(
        answer["result"],
        json!({ "tools": [{ "name": "echo", "description": "Says it back", "inputSchema": { "type": "object" } }] })
    );
    let paged = server.ask(2, "tools/list", json!({ "cursor": "next" }));
    assert_eq!(paged["error"]["code"], json!(-32602));
    server.finish().unwrap();
}

#[test]
fn a_call_reaches_the_handler_with_its_arguments_and_client() {
    let server = Running::start();
    server.initialize();
    let answer = server.ask(
        1,
        "tools/call",
        json!({ "name": "echo", "arguments": { "word": "hi" } }),
    );
    assert_eq!(
        answer["result"],
        json!({
            "content": [{ "type": "text", "text": "echoed" }],
            "structuredContent": { "word": "hi" },
        })
    );
    let call = server.calls.recv_timeout(WAIT).unwrap();
    assert_eq!(call.client, Some(ClientInfo::new("Test Client")));
    let bare = server.ask(2, "tools/call", json!({ "name": "echo" }));
    assert_eq!(bare["result"]["structuredContent"], json!({}));
    server.finish().unwrap();
}

#[test]
fn an_unknown_tool_or_malformed_call_is_a_protocol_error() {
    let server = Running::start();
    server.initialize();
    let unknown = server.ask(1, "tools/call", json!({ "name": "nope\u{1b}[2J" }));
    assert_eq!(unknown["error"]["code"], json!(-32602));
    assert_eq!(unknown["error"]["message"], json!("Unknown tool: nope[2J"));
    let nameless = server.ask(2, "tools/call", json!({ "arguments": {} }));
    assert_eq!(nameless["error"]["code"], json!(-32602));
    let listed = server.ask(3, "tools/call", json!({ "name": "echo", "arguments": [1] }));
    assert_eq!(listed["error"]["code"], json!(-32602));
    server.finish().unwrap();
}

#[test]
fn an_answer_over_the_bound_becomes_a_tool_error() {
    let server = Running::start();
    server.initialize();
    let answer = server.ask(1, "tools/call", json!({ "name": "huge" }));
    assert_eq!(answer["result"]["isError"], json!(true));
    let text = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("exceeds"), "{text}");
    server.finish().unwrap();
}

#[test]
fn every_answer_fits_the_bound_with_its_id() {
    use crate::jsonrpc::MAX_ID;
    let server = Running::start();
    server.initialize();
    // The longest ID with escapes, which grow it sixfold on the wire.
    let id = "\u{1}".repeat(MAX_ID);
    for name in ["huge", "echo"] {
        server.send(json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name } }));
        let answer = server.receive();
        assert_eq!(answer["id"], json!(id));
        assert!(serde_json::to_vec(&answer).unwrap().len() <= crate::MAX_MESSAGE);
    }
    server.finish().unwrap();
}
