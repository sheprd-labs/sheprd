//! A session through the relay as a byte stream: binary frames read and
//! written through `AsyncRead` and `AsyncWrite`, with WebSocket pings to
//! keep it from idling and, optionally, the protocol's message framing.

use super::{Config, Error, Result, connect::Socket};
use crate::framing::{self, Deframer};
use futures_util::{Sink, Stream};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf},
    time::{Interval, MissedTickBehavior, interval_at},
};
use tokio_tungstenite::tungstenite::{self, Bytes, Message};

/// One end of a session: what the far side writes is read here, and what
/// is written here reaches it, as binary frames of at most the config's
/// `max_frame` bytes each.
///
/// While the session is read, it sends a WebSocket ping every
/// `keepalive`: a relay counts pings as activity, so a quiet session
/// stays open with nothing sent inside it. A relay older than that rule
/// counts only data, so a peer that must stay quiet for longer than such a
/// relay's idle timeout keeps a keepalive of its own inside the stream.
#[derive(Debug)]
pub struct Session {
    closed: bool,
    keepalive: Option<Interval>,
    max_frame: usize,
    /// A ping is due but the socket was not ready to take it.
    ping_due: bool,
    /// What the last frame held beyond what was read.
    read: Bytes,
    socket: Socket,
}

impl Session {
    pub(crate) fn new(socket: Socket, config: &Config) -> Self {
        let keepalive = config.keepalive.map(|every| {
            let every = every.max(Duration::from_millis(100));
            let mut ticks = interval_at(tokio::time::Instant::now() + every, every);
            ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticks
        });
        Self {
            closed: false,
            keepalive,
            max_frame: config.max_frame.max(1),
            ping_due: false,
            read: Bytes::new(),
            socket,
        }
    }

    /// Sends a WebSocket ping now.
    pub async fn ping(&mut self) -> Result<()> {
        futures_util::SinkExt::send(&mut self.socket, Message::Ping(Bytes::new()))
            .await
            .map_err(super::connect::websocket)
    }

    /// Reads and writes whole messages instead of bytes ([`crate::framing`]),
    /// refusing a message over `cap` bytes.
    #[must_use]
    pub fn framed(self, cap: usize) -> Framed {
        Framed {
            deframer: Deframer::new(cap),
            session: self,
        }
    }

    /// Sends a ping if one is due, without waiting for the socket.
    fn poll_keepalive(&mut self, cx: &mut Context<'_>) {
        if let Some(keepalive) = &mut self.keepalive {
            while keepalive.poll_tick(cx).is_ready() {
                self.ping_due = true;
            }
        }
        if !self.ping_due {
            return;
        }
        let mut socket = Pin::new(&mut self.socket);
        if let Poll::Ready(Ok(())) = socket.as_mut().poll_ready(cx)
            && socket
                .as_mut()
                .start_send(Message::Ping(Bytes::new()))
                .is_ok()
        {
            self.ping_due = false;
            // Flushing may wait on the network; the next read or write
            // flushes again.
            let _ = socket.poll_flush(cx);
        }
    }
}

fn io_error(error: tungstenite::Error) -> io::Error {
    match error {
        tungstenite::Error::Io(error) => error,
        error => io::Error::other(Error::WebSocket(Box::new(error))),
    }
}

impl AsyncRead for Session {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.read.is_empty() {
                let count = this.read.len().min(buf.remaining());
                buf.put_slice(&this.read[..count]);
                this.read = this.read.slice(count..);
                return Poll::Ready(Ok(()));
            }
            if this.closed {
                return Poll::Ready(Ok(()));
            }
            this.poll_keepalive(cx);
            match ready!(Pin::new(&mut this.socket).poll_next(cx)) {
                Some(Ok(Message::Binary(bytes))) => this.read = bytes,
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                Some(Ok(Message::Text(_))) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        Error::Malformed,
                    )));
                }
                Some(Ok(Message::Close(_))) | None => this.closed = true,
                Some(Err(
                    tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed,
                )) => this.closed = true,
                Some(Err(error)) => return Poll::Ready(Err(io_error(error))),
            }
        }
    }
}

impl AsyncWrite for Session {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let mut socket = Pin::new(&mut this.socket);
        ready!(socket.as_mut().poll_ready(cx)).map_err(io_error)?;
        let count = buf.len().min(this.max_frame);
        socket
            .start_send(Message::Binary(Bytes::copy_from_slice(&buf[..count])))
            .map_err(io_error)?;
        Poll::Ready(Ok(count))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().socket)
            .poll_flush(cx)
            .map_err(io_error)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match ready!(Pin::new(&mut self.get_mut().socket).poll_close(cx)) {
            Ok(())
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                Poll::Ready(Ok(()))
            }
            Err(error) => Poll::Ready(Err(io_error(error))),
        }
    }
}

/// A session read and written as whole messages: each a big-endian `u16`
/// length, then the message ([`crate::framing`]).
#[derive(Debug)]
pub struct Framed {
    deframer: Deframer,
    session: Session,
}

impl Framed {
    /// Sends one message.
    pub async fn send(&mut self, message: &[u8]) -> Result<()> {
        let framed = framing::encode(message)?;
        self.session.write_all(&framed).await.map_err(Error::Io)?;
        self.session.flush().await.map_err(Error::Io)
    }

    /// The next message, or `None` once the far side ended the session
    /// between messages.
    ///
    /// Cancel-safe: bytes read before the future is dropped stay with this
    /// value for the next call.
    pub async fn recv(&mut self) -> Result<Option<Vec<u8>>> {
        let mut chunk = vec![0; 16 * 1024];
        loop {
            if let Some(message) = self.deframer.next_message()? {
                return Ok(Some(message));
            }
            let read = self.session.read(&mut chunk).await.map_err(Error::Io)?;
            if read == 0 {
                return match self.deframer.buffered() {
                    0 => Ok(None),
                    _ => Err(Error::Truncated),
                };
            }
            if let Some(message) = self.deframer.push(&chunk[..read])? {
                return Ok(Some(message));
            }
        }
    }

    /// The session underneath, with any partial message dropped.
    #[must_use]
    pub fn into_inner(self) -> Session {
        self.session
    }
}
