#![allow(clippy::unwrap_used, clippy::expect_used)]

mod decode;
mod lifecycle;
mod line;
mod server;
mod tools;

use crate::{Call, Handler, ServerInfo, Tool, ToolResult, UnknownTool, serve};
use serde_json::{Value, json};
use std::{
    io::{self, BufReader, Read, Write},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

/// How long a test waits for something it is owed before failing.
const WAIT: Duration = Duration::from_secs(10);

fn info() -> ServerInfo {
    ServerInfo {
        name: "sheprd".into(),
        title: Some("Sheprd".into()),
        version: "1.2.3".into(),
    }
}

/// One tool, `echo`, answering with its arguments. A call to `block`
/// reports it started, then waits for the test's go-ahead.
struct Echo {
    started: Option<Sender<()>>,
    go: Option<Receiver<()>>,
    calls: Sender<Call>,
}

impl Handler for Echo {
    fn instructions(&mut self) -> Option<String> {
        Some("Call echo.".into())
    }

    fn tools(&mut self) -> Vec<Tool> {
        vec![Tool {
            name: "echo".into(),
            title: None,
            description: "Says it back".into(),
            input_schema: json!({ "type": "object" }),
            annotations: None,
        }]
    }

    fn call(&mut self, call: Call) -> Result<ToolResult, UnknownTool> {
        let _ = self.calls.send(call.clone());
        match call.name.as_str() {
            "echo" => Ok(ToolResult::structured("echoed", call.arguments)),
            "huge" => Ok(ToolResult::text("x".repeat(crate::MAX_MESSAGE))),
            "block" => {
                if let Some(started) = &self.started {
                    let _ = started.send(());
                }
                if let Some(go) = &self.go {
                    let _ = go.recv_timeout(WAIT);
                }
                Ok(ToolResult::text("unblocked"))
            }
            _ => Err(UnknownTool),
        }
    }
}

/// Input the test feeds a line at a time; dropping the sender ends it.
struct Lines {
    lines: Receiver<Vec<u8>>,
    current: io::Cursor<Vec<u8>>,
}

impl Read for Lines {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            let read = self.current.read(buffer)?;
            if read > 0 {
                return Ok(read);
            }
            match self.lines.recv() {
                Ok(line) => self.current = io::Cursor::new(line),
                Err(_) => return Ok(0),
            }
        }
    }
}

/// Output split into messages, one per line written.
struct Messages(Sender<Value>, Vec<u8>);

impl Write for Messages {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.1.extend_from_slice(bytes);
        while let Some(end) = self.1.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = self.1.drain(..=end).collect();
            let value = serde_json::from_slice(&line).map_err(io::Error::other)?;
            let _ = self.0.send(value);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A server running on its own thread, driven a line at a time.
struct Running {
    input: Option<Sender<Vec<u8>>>,
    output: Receiver<Value>,
    calls: Receiver<Call>,
    started: Receiver<()>,
    go: Sender<()>,
    server: Option<thread::JoinHandle<crate::Result<()>>>,
}

impl Running {
    fn start() -> Self {
        let (input, lines) = mpsc::channel();
        let (sent, output) = mpsc::channel();
        let (call_sender, calls) = mpsc::channel();
        let (started_sender, started) = mpsc::channel();
        let (go, go_receiver) = mpsc::channel();
        let handler = Echo {
            started: Some(started_sender),
            go: Some(go_receiver),
            calls: call_sender,
        };
        let reader = BufReader::new(Lines {
            lines,
            current: io::Cursor::new(Vec::new()),
        });
        let server =
            thread::spawn(move || serve(info(), reader, Messages(sent, Vec::new()), handler));
        Self {
            input: Some(input),
            output,
            calls,
            started,
            go,
            server: Some(server),
        }
    }

    fn send_raw(&self, line: &[u8]) {
        self.input.as_ref().unwrap().send(line.to_vec()).unwrap();
    }

    fn send(&self, message: Value) {
        let mut line = serde_json::to_vec(&message).unwrap();
        line.push(b'\n');
        self.send_raw(&line);
    }

    fn request(&self, id: i64, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    }

    fn receive(&self) -> Value {
        self.output
            .recv_timeout(WAIT)
            .expect("a message from the server")
    }

    /// Sends a request and returns its answer, which must come next.
    fn ask(&self, id: i64, method: &str, params: Value) -> Value {
        self.request(id, method, params);
        let answer = self.receive();
        assert_eq!(answer["id"], json!(id), "{answer}");
        answer
    }

    fn initialize(&self) -> Value {
        self.ask(
            0,
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "clientInfo": { "name": "Test Client", "version": "1" } }),
        )
    }

    /// Ends the input and returns what the server returned, checking it
    /// wrote nothing more.
    fn finish(mut self) -> crate::Result<()> {
        drop(self.input.take());
        let result = self.server.take().unwrap().join().unwrap();
        assert!(
            self.output.try_recv().is_err(),
            "nothing after the input ended"
        );
        result
    }
}

/// Serves `input` in full and returns every message written.
fn serve_all(input: &[u8]) -> Vec<Value> {
    let (sent, output) = mpsc::channel();
    let (calls, _) = mpsc::channel();
    let handler = Echo {
        started: None,
        go: None,
        calls,
    };
    serve(
        info(),
        io::Cursor::new(input.to_vec()),
        Messages(sent, Vec::new()),
        handler,
    )
    .unwrap();
    output.try_iter().collect()
}
