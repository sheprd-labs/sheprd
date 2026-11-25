//! JSON-RPC 2.0 over the agent's standard input and output, one message per line.
//!
//! The reader thread classifies each line and hands it on; the writer thread
//! writes queued lines in order. Correlating responses with requests is the
//! session's job, since it is the only one that knows what each request was for.

use std::{
    io::{BufRead, BufReader, Read, Write},
    thread,
};

use acp::v1 as wire;
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{Error, Result};

/// The longest line read whole before it is parsed. A longer one is parsed as
/// it streams, so a line of any length costs only what is kept of it, never
/// its raw bytes besides.
pub(crate) const BUFFERED_LINE: usize = 1024 * 1024;
/// Lines waiting to be written to the agent.
const OUTBOX: usize = 1024;

#[derive(Debug)]
pub(crate) enum Inbound {
    Request {
        id: wire::RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: wire::RequestId,
        result: std::result::Result<Value, wire::Error>,
    },
    /// The agent closed its output, or it could not be read.
    Closed(Option<Error>),
}

/// Classifies one line; `None` for anything that is not a JSON-RPC message.
pub(crate) fn classify(line: &[u8]) -> Option<Inbound> {
    classify_value(serde_json::from_slice(line).ok()?)
}

fn classify_value(message: Value) -> Option<Inbound> {
    let Value::Object(mut message) = message else {
        return None;
    };
    let id = message
        .remove("id")
        .and_then(|id| serde_json::from_value::<wire::RequestId>(id).ok());
    let params = message.remove("params").unwrap_or(Value::Null);
    if let Some(Value::String(method)) = message.remove("method") {
        return Some(match id {
            Some(id) => Inbound::Request { id, method, params },
            None => Inbound::Notification { method, params },
        });
    }
    let id = id?;
    if let Some(error) = message.remove("error") {
        let error = serde_json::from_value(error).unwrap_or_else(|_| wire::Error::internal_error());
        return Some(Inbound::Response {
            id,
            result: Err(error),
        });
    }
    Some(Inbound::Response {
        id,
        result: Ok(message.remove("result").unwrap_or(Value::Null)),
    })
}

/// Reads the agent's output on its own thread until it ends or `deliver`
/// reports nobody is listening.
pub(crate) fn spawn_reader(
    output: impl Read + Send + 'static,
    mut deliver: impl FnMut(Inbound) -> bool + Send + 'static,
) -> std::io::Result<thread::JoinHandle<()>> {
    spawn_lines(output, "acp-reader", move |read| match read {
        Lines::Line(line) => classify(line).is_none_or(&mut deliver),
        Lines::Long(line) => serde_json::from_reader(line)
            .ok()
            .and_then(classify_value)
            .is_none_or(&mut deliver),
        Lines::End(error) => deliver(Inbound::Closed(error)),
    })
}

/// What [`spawn_lines`] hands on.
pub(crate) enum Lines<'a> {
    Line(&'a [u8]),
    /// A line longer than [`BUFFERED_LINE`], to read as it streams. Whatever
    /// of it is left unread is skipped.
    Long(&'a mut dyn Read),
    /// The output ended, cleanly or with why it could not be read.
    End(Option<Error>),
}

/// Reads lines on a thread named `name`, handing each to `handle` until it
/// returns `false`, then how the output ended unless `handle` stopped first.
/// No line is too long: one past [`BUFFERED_LINE`] is handed on as a stream.
pub(crate) fn spawn_lines(
    output: impl Read + Send + 'static,
    name: &str,
    mut handle: impl FnMut(Lines<'_>) -> bool + Send + 'static,
) -> std::io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name(name.to_owned()).spawn(move || {
        let mut reader = BufReader::new(output);
        let mut line = Vec::new();
        let error = loop {
            line.clear();
            let read = (&mut reader)
                .take(BUFFERED_LINE as u64 + 1)
                .read_until(b'\n', &mut line);
            let handled = match read {
                Ok(0) => break None,
                Ok(_) if line.len() > BUFFERED_LINE && line.last() != Some(&b'\n') => {
                    let mut rest = LineRest::new(&mut reader);
                    // Buffered again, since a parser reads a stream a byte at a time.
                    let handled = handle(Lines::Long(&mut BufReader::new(
                        (&line[..]).chain(&mut rest),
                    )));
                    if let Err(error) = rest.skip() {
                        break Some(error.into());
                    }
                    handled
                }
                Ok(_) => handle(Lines::Line(&line)),
                Err(error) => break Some(error.into()),
            };
            if !handled {
                return;
            }
        };
        handle(Lines::End(error));
    })
}

/// The rest of a line, up to and including its newline, which is consumed
/// but not read out.
struct LineRest<'a, R> {
    reader: &'a mut R,
    ended: bool,
}

impl<'a, R: BufRead> LineRest<'a, R> {
    fn new(reader: &'a mut R) -> Self {
        Self {
            reader,
            ended: false,
        }
    }

    /// Consumes what is left of the line without keeping it.
    fn skip(&mut self) -> std::io::Result<()> {
        std::io::copy(self, &mut std::io::sink()).map(drop)
    }
}

impl<R: BufRead> Read for LineRest<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.ended || out.is_empty() {
            return Ok(0);
        }
        let available = self.reader.fill_buf()?;
        if available.is_empty() {
            self.ended = true;
            return Ok(0);
        }
        let window = &available[..available.len().min(out.len())];
        let (take, consume) = match window.iter().position(|byte| *byte == b'\n') {
            Some(end) => {
                self.ended = true;
                (end, end + 1)
            }
            None => (window.len(), window.len()),
        };
        out[..take].copy_from_slice(&available[..take]);
        self.reader.consume(consume);
        Ok(take)
    }
}

/// Writes queued lines to the agent's input in order, until the queue closes.
pub(crate) fn spawn_writer(
    mut input: impl Write + Send + 'static,
) -> std::io::Result<(Outbox, thread::JoinHandle<()>)> {
    let (sender, receiver): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(OUTBOX);
    let handle = thread::Builder::new()
        .name("acp-writer".to_owned())
        .spawn(move || {
            for line in receiver {
                if input.write_all(&line).and_then(|()| input.flush()).is_err() {
                    return;
                }
            }
        })?;
    Ok((Outbox(sender), handle))
}

/// The queue of lines for the agent. Never blocks: an agent that stops
/// reading its input is reported rather than waited on.
#[derive(Clone, Debug)]
pub(crate) struct Outbox(pub(crate) Sender<Vec<u8>>);

impl Outbox {
    /// Queues one message as a line.
    pub(crate) fn send(&self, message: &Value) -> Result<()> {
        let mut line = serde_json::to_vec(message).map_err(Error::Encode)?;
        line.push(b'\n');
        match self.0.try_send(line) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => Err(Error::AgentStalled),
        }
    }

    pub(crate) fn request(&self, id: i64, method: &str, params: &impl Serialize) -> Result<()> {
        let params = serde_json::to_value(params).map_err(Error::Encode)?;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    pub(crate) fn notify(&self, method: &str, params: &impl Serialize) -> Result<()> {
        let params = serde_json::to_value(params).map_err(Error::Encode)?;
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    pub(crate) fn respond(
        &self,
        id: &wire::RequestId,
        result: std::result::Result<Value, wire::Error>,
    ) -> Result<()> {
        let message = match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        self.send(&message)
    }
}

#[cfg(test)]
mod tests;
