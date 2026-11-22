use super::*;
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_client_initializes_lists_and_calls_over_stdio() {
    let lines = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": "Cursor" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "memory_search", "arguments": { "query": "build" } } }),
    ];
    let input: Vec<u8> = lines
        .iter()
        .flat_map(|line| {
            let mut bytes = serde_json::to_vec(line).unwrap();
            bytes.push(b'\n');
            bytes
        })
        .collect();
    let output = Shared::default();
    let (handler, ran) = sheprd(true, done);
    sheprd_mcp::serve(
        server_info(),
        io::Cursor::new(input),
        output.clone(),
        handler,
    )
    .unwrap();
    let bytes = output.0.lock().unwrap().clone();
    let answers: Vec<Value> = bytes
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(answers.len(), 3, "{answers:?}");
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "sheprd");
    assert_eq!(
        answers[1]["result"]["tools"].as_array().unwrap().len(),
        tools::all().count()
    );
    assert_eq!(answers[2]["result"]["content"][0]["text"], "done");
    assert!(matches!(
        ran.borrow().as_slice(),
        [(
            ControlCommand::Memory(crate::cli::control::MemoryCommand::Search { .. }),
            None
        )]
    ));
    crate::control::relay_as(None);
}
