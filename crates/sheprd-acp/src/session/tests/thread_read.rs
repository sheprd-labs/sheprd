//! The server that reads the chat's thread: given to ACP agents in their
//! sessions, and named by the handoff when entries are left out.

use crossbeam_channel::bounded;
use sheprd_chat_protocol::thread::ThreadId;

use super::harnesses::{requests, set_agent};
use super::kept::work;
use super::threads::{respond_as, sent_prompt};
use super::*;
use crate::thread_mcp::{Given, Server};

fn server() -> Server {
    Server {
        args: crate::thread_mcp::args(ThreadId::random()),
        program: "/bin/sheprd-acp".to_owned(),
    }
}

/// The `mcpServers` a session request carried.
fn servers(request: &Value) -> Vec<Value> {
    request["params"]["mcpServers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A chat keeping a thread in `dir` whose agent takes the server in its
/// session, with its session `s1` open.
fn reading(dir: &std::path::Path) -> Harness {
    let mut harness = harness_with(Config {
        choices: Some(vec![Agent::Claude, Agent::Codex]),
        cwd: work(),
        given: Given::InSession(server()),
        thread: Some(ThreadStart::read(dir.to_owned(), ThreadId::random()).unwrap()),
        ..config()
    });
    harness.session.start();
    let initialize = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        initialize,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let new = harness.last_request("session/new");
    let given = servers(&new);
    assert_eq!(given.len(), 1);
    assert_eq!(given[0]["name"], "sheprd-thread");
    assert_eq!(given[0]["command"], "/bin/sheprd-acp");
    assert_eq!(given[0]["args"][0], "thread-mcp");
    assert_eq!(given[0]["args"][1], "--thread");
    harness.respond(new["id"].as_i64().unwrap(), json!({"sessionId": "s1"}));
    harness
}

#[test]
fn an_agent_without_the_server_gets_no_mcp_servers() {
    let mut harness = harness_with(config());
    harness.session.start();
    let initialize = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        initialize,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    assert!(servers(&harness.last_request("session/new")).is_empty());
}

#[test]
fn a_long_conversation_handed_over_names_what_thread_read_can_read() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = reading(dir.path());
    // More than the handoff's budget, a turn at a time.
    for at in 0..24 {
        let turn = prompt(&mut harness, 1, &format!("step {at}"));
        harness.agent_says(&"x".repeat(4000));
        harness.respond(turn, json!({"stopReason": "end_turn"}));
    }
    let (flow, _) = set_agent(&mut harness, Agent::Codex);
    assert!(matches!(flow, Flow::Launch { .. }));
    let (sender, codex) = bounded(64);
    harness
        .session
        .launched(Outbox(sender), 7, Given::InSession(server()));
    let initialize = requests(&codex)[0]["id"].as_i64().unwrap();
    respond_as(
        &mut harness,
        1,
        initialize,
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let open = requests(&codex).remove(0);
    assert_eq!(open["method"], "session/new");
    assert_eq!(servers(&open).len(), 1);
    respond_as(
        &mut harness,
        1,
        open["id"].as_i64().unwrap(),
        json!({"sessionId": "c1"}),
    );

    let (_, blocks) = sent_prompt(&mut harness, &codex, "go on");
    let handoff = &blocks[0];
    assert!(
        handoff.contains("read them with the thread_read tool"),
        "{handoff}"
    );
    assert!(handoff.contains(" left out: #"), "{handoff}");
}
