use super::*;
use crate::{ClientInfo, MAX_CLIENT_NAME, PROTOCOL_VERSIONS};

#[test]
fn initialize_reports_the_server_and_its_tools_capability() {
    let server = Running::start();
    let answer = server.initialize();
    assert_eq!(
        answer["result"],
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "sheprd", "title": "Sheprd", "version": "1.2.3" },
            "instructions": "Call echo.",
        })
    );
    server.finish().unwrap();
}

#[test]
fn every_supported_version_is_echoed_and_others_get_the_newest() {
    for version in PROTOCOL_VERSIONS {
        let server = Running::start();
        let answer = server.ask(1, "initialize", json!({ "protocolVersion": version }));
        assert_eq!(answer["result"]["protocolVersion"], json!(version));
        server.finish().unwrap();
    }
    let server = Running::start();
    let answer = server.ask(1, "initialize", json!({ "protocolVersion": "2024-11-05" }));
    assert_eq!(
        answer["result"]["protocolVersion"],
        json!(PROTOCOL_VERSIONS[0])
    );
    server.finish().unwrap();
}

#[test]
fn initialize_needs_a_version_and_happens_once() {
    let server = Running::start();
    let answer = server.ask(1, "initialize", json!({}));
    assert_eq!(answer["error"]["code"], json!(-32602));
    server.initialize();
    let again = server.ask(2, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert_eq!(again["error"]["code"], json!(-32600));
    server.finish().unwrap();
}

#[test]
fn tools_wait_for_initialize_but_ping_does_not() {
    let server = Running::start();
    assert_eq!(
        server.ask(1, "tools/list", json!({}))["error"]["code"],
        json!(-32600)
    );
    assert_eq!(server.ask(2, "ping", json!({}))["result"], json!({}));
    server.initialize();
    assert_eq!(
        server.ask(3, "resources/list", json!({}))["error"]["code"],
        json!(-32601)
    );
    server.finish().unwrap();
}

#[test]
fn notifications_get_no_answer() {
    let server = Running::start();
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/unknown", "params": {} }));
    server.send(json!({ "jsonrpc": "2.0", "id": 99, "result": {} }));
    // The ping's answer comes first: nothing above was answered.
    assert_eq!(server.ask(1, "ping", json!({}))["result"], json!({}));
    server.finish().unwrap();
}

#[test]
fn client_names_are_kept_printable_and_bounded() {
    assert_eq!(ClientInfo::new("  Cursor\u{7}\n ").name, "Cursor");
    let long = "é".repeat(MAX_CLIENT_NAME);
    let kept = ClientInfo::new(&long).name;
    assert!(kept.len() <= MAX_CLIENT_NAME && kept.len() >= MAX_CLIENT_NAME - 1);
    assert!(kept.chars().all(|character| character == 'é'));
}
