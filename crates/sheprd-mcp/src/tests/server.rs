use super::*;
use crate::{MAX_MESSAGE, server::QUEUE};

#[test]
fn ping_is_answered_while_a_call_runs() {
    let server = Running::start();
    server.initialize();
    server.request(1, "tools/call", json!({ "name": "block" }));
    server.started.recv_timeout(WAIT).unwrap();
    assert_eq!(server.ask(2, "ping", json!({}))["result"], json!({}));
    server.go.send(()).unwrap();
    assert_eq!(server.receive()["id"], json!(1));
    server.finish().unwrap();
}

#[test]
fn a_cancelled_call_gets_no_answer_and_the_next_one_does() {
    let server = Running::start();
    server.initialize();
    server.request(1, "tools/call", json!({ "name": "block" }));
    server.started.recv_timeout(WAIT).unwrap();
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 1 } }));
    // The reader handles messages in order, so the ping's answer shows the
    // cancellation was recorded before the call is let go.
    server.ask(2, "ping", json!({}));
    server.go.send(()).unwrap();
    let next = server.ask(3, "tools/call", json!({ "name": "echo" }));
    assert_eq!(next["result"]["content"][0]["text"], json!("echoed"));
    server.finish().unwrap();
}

#[test]
fn cancelling_an_unknown_request_changes_nothing() {
    let server = Running::start();
    server.initialize();
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 5 } }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": {} }));
    let answer = server.ask(5, "tools/call", json!({ "name": "echo" }));
    assert!(answer.get("result").is_some(), "{answer}");
    server.finish().unwrap();
}

#[test]
fn calls_run_one_at_a_time_in_order_and_a_full_queue_is_refused() {
    let server = Running::start();
    server.initialize();
    server.request(1, "tools/call", json!({ "name": "block" }));
    server.started.recv_timeout(WAIT).unwrap();
    for id in 0..QUEUE {
        server.request(10 + id as i64, "tools/call", json!({ "name": "echo" }));
    }
    let busy = server.ask(100, "tools/call", json!({ "name": "echo" }));
    assert_eq!(busy["error"]["code"], json!(-32000));
    let duplicate = server.ask(10, "tools/call", json!({ "name": "echo" }));
    assert_eq!(duplicate["error"]["code"], json!(-32600));
    server.go.send(()).unwrap();
    assert_eq!(server.receive()["id"], json!(1));
    for id in 0..QUEUE {
        assert_eq!(server.receive()["id"], json!(10 + id as i64));
    }
    // A finished ID may be used again.
    assert!(
        server
            .ask(10, "tools/call", json!({ "name": "echo" }))
            .get("result")
            .is_some()
    );
    server.finish().unwrap();
}

#[test]
fn malformed_and_oversized_lines_are_answered_and_serving_goes_on() {
    let mut input = b"{oops\n\n   \n".to_vec();
    input.extend(vec![b' '; MAX_MESSAGE + 10]);
    input.extend_from_slice(b"x\n");
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    let messages = serve_all(&input);
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert_eq!(messages[0]["error"]["code"], json!(-32700));
    assert_eq!(messages[0]["id"], Value::Null);
    assert_eq!(messages[1]["error"]["code"], json!(-32600));
    assert_eq!(
        messages[2],
        json!({ "jsonrpc": "2.0", "id": 1, "result": {} })
    );
}

#[test]
fn queued_calls_are_answered_before_serve_returns() {
    let lines = [
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-11-25" } }),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "echo" } }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "echo" } }),
    ];
    let input: Vec<u8> = lines
        .iter()
        .flat_map(|line| {
            let mut bytes = serde_json::to_vec(line).unwrap();
            bytes.push(b'\n');
            bytes
        })
        .collect();
    let ids: Vec<Value> = serve_all(&input)
        .into_iter()
        .map(|message| message["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!(0), json!(1), json!(2)]);
}

#[test]
fn a_failed_write_stops_the_server_with_its_cause() {
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (calls, _) = mpsc::channel();
    let handler = Echo {
        started: None,
        go: None,
        calls,
    };
    let input = br#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#;
    let error = serve(info(), io::Cursor::new(input.to_vec()), Closed, handler).unwrap_err();
    let crate::Error::Write(source) = error else {
        panic!("expected a write error, got {error:?}");
    };
    assert_eq!(source.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn a_call_cancelled_while_it_waits_never_runs() {
    let server = Running::start();
    server.initialize();
    server.request(1, "tools/call", json!({ "name": "block" }));
    server.started.recv_timeout(WAIT).unwrap();
    assert_eq!(server.calls.recv_timeout(WAIT).unwrap().name, "block");
    server.request(2, "tools/call", json!({ "name": "echo" }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 2 } }));
    // In order on the reader: the cancellation is recorded once this answers.
    server.ask(3, "ping", json!({}));
    server.go.send(()).unwrap();
    assert_eq!(server.receive()["id"], json!(1));
    // The next call's answer comes straight after: 2 was skipped, unanswered.
    let next = server.ask(4, "tools/call", json!({ "name": "echo" }));
    assert!(next.get("result").is_some(), "{next}");
    let ran: Vec<String> = server.calls.try_iter().map(|call| call.name).collect();
    assert_eq!(ran, ["echo"], "only call 4 reached the handler");
    server.finish().unwrap();
}
