//! The stdio loop: a reader thread takes messages off the input, answers
//! what needs no tool (`ping`, malformed messages, cancellations), and
//! queues the rest for the calling thread, which runs them one at a time.

use crate::{
    Error, Handler, MAX_MESSAGE, Result, ServerInfo,
    jsonrpc::{self, Failure, INVALID_REQUEST, Id, Incoming, Refusal, Request, SERVER_BUSY},
    line::{self, Line},
    session::Session,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{BufRead, Write},
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{self, TrySendError},
    },
    thread,
};

/// Requests that may wait behind the one running. More are refused with
/// [`SERVER_BUSY`] rather than buffered.
pub(crate) const QUEUE: usize = 16;

/// The requests queued or running, so a cancellation names a real one and
/// a reused ID is refused. Both sets hold at most `QUEUE + 1` IDs.
#[derive(Default)]
struct Pending {
    ids: HashSet<Id>,
    cancelled: HashSet<Id>,
}

/// Serves `input` until it ends, answering on `output`. Tool calls run on
/// the calling thread, so `handler` need not be `Send`. Returns once every
/// request read has been answered or cancelled.
pub fn serve<R, W>(info: ServerInfo, input: R, output: W, mut handler: impl Handler) -> Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let output = Arc::new(Mutex::new(output));
    let pending = Arc::new(Mutex::new(Pending::default()));
    let (requests, queue) = mpsc::sync_channel(QUEUE);
    let reader = {
        let output = Arc::clone(&output);
        let pending = Arc::clone(&pending);
        thread::Builder::new()
            .name("mcp-input".into())
            .spawn(move || read(input, &output, &pending, &requests))
            .map_err(Error::Spawn)?
    };
    let mut session = Session::new(info);
    for request in queue {
        let id = request.id.clone();
        // A request cancelled while it waited never runs: a tool that
        // changes something must not act for a call its client gave up on.
        if lock(&pending).cancelled.contains(&id) {
            finish(&pending, &id);
            continue;
        }
        let response = session.answer(request, &mut handler);
        // One cancelled while it ran finishes, but gets no answer, as the
        // protocol asks.
        if !finish(&pending, &id) {
            write(&output, &response)?;
        }
    }
    reader.join().map_err(|_| Error::ReaderPanicked)?
}

fn read<W: Write>(
    mut input: impl BufRead,
    output: &Mutex<W>,
    pending: &Mutex<Pending>,
    requests: &mpsc::SyncSender<Request>,
) -> Result<()> {
    let mut buffer = Vec::new();
    loop {
        let message = match line::read(&mut input, &mut buffer, MAX_MESSAGE).map_err(Error::Read)? {
            Line::End => return Ok(()),
            Line::TooLong => {
                let failure = Failure::new(
                    INVALID_REQUEST,
                    format!("A message exceeds {MAX_MESSAGE} bytes"),
                );
                write(output, &jsonrpc::error(None, &failure))?;
                continue;
            }
            Line::Read if buffer.iter().all(u8::is_ascii_whitespace) => continue,
            Line::Read => jsonrpc::decode(&buffer),
        };
        match message {
            Err(refusal) => write(output, &Value::from(&refusal))?,
            Ok(Incoming::Request(request)) if request.method == "ping" => {
                write(output, &jsonrpc::result(&request.id, json!({})))?;
            }
            Ok(Incoming::Request(request)) => {
                if !lock(pending).ids.insert(request.id.clone()) {
                    let refusal = Refusal {
                        id: Some(request.id),
                        failure: Failure::new(
                            INVALID_REQUEST,
                            "A request with this id is still pending",
                        ),
                    };
                    write(output, &Value::from(&refusal))?;
                    continue;
                }
                let id = request.id.clone();
                match requests.try_send(request) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        lock(pending).ids.remove(&id);
                        let failure = Failure::new(SERVER_BUSY, "Too many requests are waiting");
                        write(output, &jsonrpc::error(Some(&id), &failure))?;
                    }
                    // The calling thread stopped, having failed to write.
                    Err(TrySendError::Disconnected(_)) => return Ok(()),
                }
            }
            Ok(Incoming::Notification { method, params }) => {
                if method == "notifications/cancelled" {
                    cancel(
                        pending,
                        params.as_ref().and_then(|params| params.get("requestId")),
                    );
                }
            }
            Ok(Incoming::Response) => {}
        }
    }
}

/// Forgets a request that ran or was skipped; whether it was cancelled.
fn finish(pending: &Mutex<Pending>, id: &Id) -> bool {
    let mut pending = lock(pending);
    pending.ids.remove(id);
    pending.cancelled.remove(id)
}

/// Marks a queued or running request so its answer is dropped. An unknown
/// or finished ID is ignored, as the protocol allows for a race.
fn cancel(pending: &Mutex<Pending>, id: Option<&Value>) {
    let Some(id) = id.and_then(|id| serde_json::from_value::<Id>(id.clone()).ok()) else {
        return;
    };
    let mut pending = lock(pending);
    if pending.ids.contains(&id) {
        pending.cancelled.insert(id);
    }
}

fn write<W: Write>(output: &Mutex<W>, message: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(message).map_err(Error::Encode)?;
    bytes.push(b'\n');
    let mut output = lock(output);
    output.write_all(&bytes).map_err(Error::Write)?;
    output.flush().map_err(Error::Write)
}

/// A panic elsewhere leaves the data whole: each holder only inserts,
/// removes, or writes one message.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
