//! One side of a multiplexed connection: its streams, the frames waiting to
//! go out, and the credit each stream may still send. The state's lock is
//! never held across I/O, so a stream closes from any thread at once.

use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use super::{CloseReason, Error, Frame, Hello, MAX_DATA, MAX_STREAMS, Result, StreamId, WINDOW};
use crate::EndpointId;

/// One side of a multiplexed connection. Clones share it.
#[derive(Clone)]
pub struct Mux {
    shared: Arc<Shared>,
}

/// One multiplexed connection to a chat. Clones share it; it stays open
/// until [`Stream::close`], which every owner calls when done.
#[derive(Clone)]
pub struct Stream {
    shared: Arc<Shared>,
    id: StreamId,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Client,
    Host,
}

struct State {
    role: Role,
    streams: HashMap<StreamId, Entry>,
    /// Frames to send, in order, costing `queued` bytes in all. A peer that
    /// stops reading would let replies and closed streams' data pile up,
    /// so past [`State::max_queued`] the connection ends instead.
    out: VecDeque<Out>,
    queued: usize,
    /// Streams the client opened that the host has not taken yet.
    accepted: VecDeque<(StreamId, EndpointId)>,
    /// The client's last opened stream; the host's last seen.
    last_id: StreamId,
    /// The most streams open at once: the host's, as its greeting says.
    max_streams: usize,
    hello: Option<Hello>,
    ended: Option<End>,
    /// Since when no stream has been open.
    idle_since: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    /// Shut down or closed by the peer between frames.
    Clean,
    Failed,
}

#[derive(Default)]
struct Entry {
    inbound: VecDeque<u8>,
    /// Bytes this side may still send.
    credit: u32,
    /// Bytes the peer sent that it has not yet been given back.
    outstanding: u32,
    /// Bytes read that a queued grant will return.
    grant: u32,
    granting: bool,
    /// Set once the peer closed the stream.
    remote: Option<CloseReason>,
}

enum Out {
    Frame(Frame),
    /// The stream's pending grant, read when it is sent.
    Grant(StreamId),
}

/// Room in a connection's queue for frames other than data, beyond every
/// stream's window: opens, grants, closes, and refusals.
const MAX_CONTROL: usize = 64 * 1024;

impl Out {
    /// Its bytes on the wire, which is what it holds while queued.
    fn cost(&self) -> usize {
        match self {
            Self::Frame(frame) => Self::cost_of(frame),
            Self::Grant(_) => HEADER + 4,
        }
    }

    fn cost_of(frame: &Frame) -> usize {
        HEADER
            + match frame {
                Frame::Hello(_) => 16,
                Frame::Open { .. } => 32,
                Frame::Data { bytes, .. } => bytes.len(),
                Frame::Credit { .. } => 4,
                Frame::Close { .. } => 1,
            }
    }
}

/// A frame's kind, stream, and length.
const HEADER: usize = 9;

impl Mux {
    /// The client side: it opens streams once the host greets.
    pub fn client() -> Self {
        Self::new(Role::Client, MAX_STREAMS, None)
    }

    /// The host side, carrying at most `max_streams` streams; its greeting
    /// is the first frame it sends.
    pub fn host(max_streams: usize) -> Self {
        Self::new(Role::Host, max_streams, Some(Frame::hello(max_streams)))
    }

    fn new(role: Role, max_streams: usize, greeting: Option<Frame>) -> Self {
        let state = State {
            role,
            streams: HashMap::new(),
            queued: greeting.as_ref().map_or(0, Out::cost_of),
            out: greeting.map(Out::Frame).into_iter().collect(),
            accepted: VecDeque::new(),
            last_id: 0,
            max_streams,
            hello: None,
            ended: None,
            idle_since: Some(Instant::now()),
        };
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                changed: Condvar::new(),
            }),
        }
    }

    /// Writes queued frames to `writer` until the connection ends, flushing
    /// whenever nothing more is queued.
    pub fn send_loop(&self, mut writer: impl Write) -> Result<()> {
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            let more = {
                let mut state = self.shared.lock();
                loop {
                    if state.ended.is_some() {
                        return Ok(());
                    }
                    match state.out.pop_front() {
                        Some(out) => {
                            state.queued = state.queued.saturating_sub(out.cost());
                            if let Some(frame) = state.frame(out) {
                                frame.encode(&mut bytes)?;
                                break;
                            }
                        }
                        None => state = self.shared.wait(state),
                    }
                }
                !state.out.is_empty()
            };
            let written = writer
                .write_all(&bytes)
                .and_then(|()| if more { Ok(()) } else { writer.flush() });
            if let Err(error) = written {
                self.end(End::Failed);
                return Err(error.into());
            }
        }
    }

    /// Reads frames from `reader` and applies them until the peer closes or
    /// breaks the protocol; either way the connection then ends.
    pub fn receive_loop(&self, mut reader: impl Read) -> Result<()> {
        let result = loop {
            match Frame::read_from(&mut reader) {
                Ok(Some(frame)) => {
                    if let Err(error) = self.apply(frame) {
                        break Err(error);
                    }
                }
                Ok(None) => break Ok(()),
                Err(error) => break Err(error),
            }
        };
        self.end(if result.is_ok() {
            End::Clean
        } else {
            End::Failed
        });
        result
    }

    fn apply(&self, frame: Frame) -> Result<()> {
        let mut state = self.shared.lock();
        let role = state.role;
        match frame {
            Frame::Hello(hello) => {
                if role != Role::Client || state.hello.is_some() {
                    return Err(Error::Unexpected(1));
                }
                if hello.version != super::VERSION {
                    return Err(Error::Version(hello.version));
                }
                state.max_streams = state.max_streams.min(hello.max_streams as usize);
                state.hello = Some(hello);
            }
            Frame::Open { stream, endpoint } => {
                if role != Role::Host {
                    return Err(Error::Unexpected(2));
                }
                if stream <= state.last_id {
                    return Err(Error::Order(stream));
                }
                state.last_id = stream;
                if state.streams.len() >= state.max_streams {
                    let refusal = Out::Frame(Frame::Close {
                        stream,
                        reason: CloseReason::Limit,
                    });
                    if !state.queue(refusal) {
                        return Err(Error::Backlog);
                    }
                } else {
                    state.add(stream);
                    state.accepted.push_back((stream, endpoint));
                }
            }
            Frame::Data { stream, bytes } => {
                let Some(entry) = state.streams.get_mut(&stream) else {
                    return Ok(());
                };
                let len = u32::try_from(bytes.len()).map_err(|_| Error::Credit(stream))?;
                match entry.outstanding.checked_add(len) {
                    Some(outstanding) if outstanding <= WINDOW && entry.remote.is_none() => {
                        entry.outstanding = outstanding;
                        entry.inbound.extend(bytes);
                    }
                    _ => return Err(Error::Credit(stream)),
                }
            }
            Frame::Credit { stream, bytes } => {
                let Some(entry) = state.streams.get_mut(&stream) else {
                    return Ok(());
                };
                match entry.credit.checked_add(bytes) {
                    Some(credit) if credit <= WINDOW => entry.credit = credit,
                    _ => return Err(Error::Credit(stream)),
                }
            }
            Frame::Close { stream, reason } => {
                if let Some(entry) = state.streams.get_mut(&stream) {
                    entry.remote.get_or_insert(reason);
                }
            }
        }
        drop(state);
        self.shared.changed.notify_all();
        Ok(())
    }

    /// Waits until the host greets, the connection ends, or `timeout`
    /// passes. A peer that ends first is no multiplexer, such as an older
    /// program refusing the subcommand.
    pub fn wait_hello(&self, timeout: Duration) -> Result<Hello> {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.lock();
        loop {
            if let Some(hello) = state.hello {
                return Ok(hello);
            }
            if state.ended.is_some() {
                return Err(Error::NotMultiplexer);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout);
            }
            state = self.shared.wait_for(state, left);
        }
    }

    /// Opens a stream to the chat at `endpoint`; the client's side only.
    pub fn open(&self, endpoint: &EndpointId) -> Result<Stream> {
        let mut state = self.shared.lock();
        if state.ended.is_some() {
            return Err(Error::Ended);
        }
        if state.role != Role::Client || state.hello.is_none() {
            return Err(Error::Unexpected(2));
        }
        if state.streams.len() >= state.max_streams {
            return Err(Error::Limit);
        }
        let id = state.last_id.checked_add(1).ok_or(Error::Limit)?;
        state.last_id = id;
        state.add(id);
        let opening = Out::Frame(Frame::Open {
            stream: id,
            endpoint: endpoint.clone(),
        });
        if !state.queue(opening) {
            drop(state);
            self.shared.changed.notify_all();
            return Err(Error::Backlog);
        }
        drop(state);
        self.shared.changed.notify_all();
        Ok(Stream {
            shared: self.shared.clone(),
            id,
        })
    }

    /// The next stream the client opened, or `None` once the connection
    /// ended; the host's side only.
    pub fn accept(&self) -> Option<(EndpointId, Stream)> {
        let mut state = self.shared.lock();
        loop {
            if state.ended.is_some() {
                return None;
            }
            if let Some((id, endpoint)) = state.accepted.pop_front() {
                let stream = Stream {
                    shared: self.shared.clone(),
                    id,
                };
                return Some((endpoint, stream));
            }
            state = self.shared.wait(state);
        }
    }

    /// Returns once the connection ended, ending it once it has carried no
    /// stream for `idle`. The check and the end happen under one lock, so
    /// a stream opened meanwhile either keeps it up or finds it ended.
    pub fn end_when_idle(&self, idle: Duration) {
        let mut state = self.shared.lock();
        loop {
            if state.ended.is_some() {
                return;
            }
            state = match state.idle_since {
                Some(since) if since.elapsed() >= idle => {
                    state.finish(End::Clean);
                    drop(state);
                    self.shared.changed.notify_all();
                    return;
                }
                Some(since) => self
                    .shared
                    .wait_for(state, idle.saturating_sub(since.elapsed())),
                None => self.shared.wait(state),
            };
        }
    }

    /// Streams open now.
    pub fn streams(&self) -> usize {
        self.shared.lock().streams.len()
    }

    pub fn is_ended(&self) -> bool {
        self.shared.lock().ended.is_some()
    }

    /// Ends the connection: streams read their end, writes fail, and the
    /// loops return. Never waits for I/O.
    pub fn shutdown(&self) {
        self.end(End::Clean);
    }

    fn end(&self, end: End) {
        self.shared.lock().finish(end);
        self.shared.changed.notify_all();
    }
}

impl std::fmt::Debug for Mux {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.shared.lock();
        formatter
            .debug_struct("Mux")
            .field("streams", &state.streams.len())
            .field("ended", &state.ended.is_some())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stream")
            .field("id", &self.id)
            .finish()
    }
}

impl State {
    /// Queues `out`, or ends the connection when it would pass
    /// [`Self::max_queued`]; false then.
    fn queue(&mut self, out: Out) -> bool {
        let queued = self.queued.saturating_add(out.cost());
        if queued > self.max_queued() {
            self.finish(End::Failed);
            return false;
        }
        self.queued = queued;
        self.out.push_back(out);
        true
    }

    /// The most bytes queued at once: every stream's full window of data,
    /// and room for the small frames that go with them.
    fn max_queued(&self) -> usize {
        self.max_streams
            .max(1)
            .saturating_mul(WINDOW as usize)
            .saturating_add(MAX_CONTROL)
    }

    fn finish(&mut self, end: End) {
        self.ended.get_or_insert(end);
        self.out.clear();
        self.queued = 0;
        self.accepted.clear();
    }

    fn add(&mut self, id: StreamId) {
        self.idle_since = None;
        self.streams.insert(
            id,
            Entry {
                credit: WINDOW,
                ..Entry::default()
            },
        );
    }

    /// The frame `out` stands for; a grant whose stream closed is none.
    fn frame(&mut self, out: Out) -> Option<Frame> {
        match out {
            Out::Frame(frame) => Some(frame),
            Out::Grant(stream) => {
                let entry = self.streams.get_mut(&stream)?;
                entry.granting = false;
                let bytes = std::mem::take(&mut entry.grant);
                entry.outstanding = entry.outstanding.saturating_sub(bytes);
                (bytes > 0).then_some(Frame::Credit { stream, bytes })
            }
        }
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn wait_for<'a>(&self, state: MutexGuard<'a, State>, time: Duration) -> MutexGuard<'a, State> {
        self.changed
            .wait_timeout(state, time)
            .map_or_else(|error| error.into_inner().0, |(state, _)| state)
    }
}

impl Stream {
    pub fn id(&self) -> StreamId {
        self.id
    }

    /// Closes the stream for every clone: reads end, writes fail, and the
    /// peer is told. Never waits for I/O.
    pub fn close(&self) {
        self.close_with(CloseReason::Ended);
    }

    /// [`Self::close`], telling the peer why.
    pub fn close_with(&self, reason: CloseReason) {
        let mut state = self.shared.lock();
        let Some(entry) = state.streams.remove(&self.id) else {
            return;
        };
        if entry.remote.is_none() && state.ended.is_none() {
            state.queue(Out::Frame(Frame::Close {
                stream: self.id,
                reason,
            }));
        }
        if state.streams.is_empty() {
            state.idle_since = Some(Instant::now());
        }
        drop(state);
        self.shared.changed.notify_all();
    }
}

/// What a stream the peer closed reads once its data is taken.
fn closed(reason: CloseReason) -> io::Result<usize> {
    match reason {
        CloseReason::Ended => Ok(0),
        CloseReason::Unreachable => Err(io::ErrorKind::ConnectionRefused.into()),
        CloseReason::Limit => Err(io::ErrorKind::QuotaExceeded.into()),
    }
}

impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut state = self.shared.lock();
        loop {
            let ended = state.ended;
            let Some(entry) = state.streams.get_mut(&self.id) else {
                return Ok(0);
            };
            if !entry.inbound.is_empty() {
                let read = entry.inbound.len().min(buffer.len());
                for (slot, byte) in buffer.iter_mut().zip(entry.inbound.drain(..read)) {
                    *slot = byte;
                }
                // At most the window, which fits a u32.
                entry.grant = entry
                    .grant
                    .saturating_add(u32::try_from(read).unwrap_or(u32::MAX));
                if !entry.granting && ended.is_none() {
                    entry.granting = true;
                    state.queue(Out::Grant(self.id));
                    self.shared.changed.notify_all();
                }
                return Ok(read);
            }
            if let Some(reason) = entry.remote {
                return closed(reason);
            }
            match ended {
                Some(End::Clean) => return Ok(0),
                Some(End::Failed) => return Err(io::ErrorKind::ConnectionAborted.into()),
                None => state = self.shared.wait(state),
            }
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut state = self.shared.lock();
        loop {
            let ended = state.ended.is_some();
            let entry = match state.streams.get_mut(&self.id) {
                Some(entry) if entry.remote.is_none() && !ended => entry,
                _ => return Err(io::ErrorKind::BrokenPipe.into()),
            };
            if entry.credit == 0 {
                state = self.shared.wait(state);
                continue;
            }
            let len = buffer
                .len()
                .min(MAX_DATA)
                .min(usize::try_from(entry.credit).unwrap_or(usize::MAX));
            // `len` is at most the credit, a u32.
            entry.credit -= u32::try_from(len).unwrap_or(entry.credit);
            let chunk = &buffer[..len];
            // Coalesce with this stream's last queued data while it fits.
            let fits = state.queued.saturating_add(len) <= state.max_queued();
            let queued = match state.out.back_mut() {
                Some(Out::Frame(Frame::Data { stream, bytes }))
                    if fits && *stream == self.id && bytes.len() + len <= MAX_DATA =>
                {
                    bytes.extend_from_slice(chunk);
                    state.queued += len;
                    true
                }
                _ => state.queue(Out::Frame(Frame::Data {
                    stream: self.id,
                    bytes: chunk.to_vec(),
                })),
            };
            drop(state);
            self.shared.changed.notify_all();
            if !queued {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            return Ok(len);
        }
    }

    /// Queued data goes out as soon as the sender gets to it.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
