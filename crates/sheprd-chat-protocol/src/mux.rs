//! Many chat connections over one byte stream, so a client can attach every
//! chat on an SSH host through one channel (`sheprd-acp multiplex`) instead
//! of a channel per chat, which sshd's `MaxSessions` and a relay's per-key
//! session cap both limit.
//!
//! The client opens streams, each naming the chat endpoint it reaches; the
//! host connects each to that endpoint and passes bytes through unread, so
//! the attach protocol runs end to end. Nothing is buffered without bound:
//! each side may send a stream at most [`WINDOW`] bytes its peer has not yet
//! returned as credit, a host takes at most [`MAX_STREAMS`] streams, and
//! frames are at most [`MAX_DATA`] bytes. One slow stream therefore never
//! stalls the others, and a peer that overruns a window ends the connection.
//!
//! [`Mux`] holds one side's state; its owner runs [`Mux::send_loop`] and
//! [`Mux::receive_loop`] on threads of its own, and each [`Stream`] reads,
//! writes, and closes from any thread. Closing never waits for I/O.

mod frame;
mod session;

use std::io;

pub use frame::{CloseReason, Frame, Hello, StreamId};
pub use session::{Mux, Stream};

/// The multiplexing protocol's version, which the host's greeting states.
pub const VERSION: u32 = 1;
/// The bytes a host's greeting starts with, so a client never mistakes a
/// shell's chatter or an older program's output for a multiplexer.
pub const MAGIC: [u8; 8] = *b"SHEPRDMX";
/// The largest data frame payload, in bytes.
pub const MAX_DATA: usize = 32 * 1024;
/// Bytes either side may send one stream before its peer returns credit:
/// the most a receiver ever buffers for that stream.
pub const WINDOW: u32 = 256 * 1024;
/// Streams a host carries at once; it refuses more.
pub const MAX_STREAMS: usize = 64;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the mux peer stopped reading")]
    Backlog,
    #[error("a mux peer sent stream {0} more than its window")]
    Credit(StreamId),
    #[error("the multiplexed connection ended")]
    Ended,
    #[error("the multiplexed connection failed")]
    Io(#[from] io::Error),
    #[error("too many multiplexed streams are open")]
    Limit,
    #[error("a malformed mux frame of kind {0}")]
    Malformed(u8),
    #[error("the peer is not a Sheprd multiplexer")]
    NotMultiplexer,
    #[error("mux stream {0} was opened out of order")]
    Order(StreamId),
    #[error("the multiplexer did not greet in time")]
    Timeout,
    #[error("a mux frame of {0} bytes exceeds the limit")]
    TooLarge(usize),
    #[error("a mux frame of kind {0} the peer may not send")]
    Unexpected(u8),
    #[error("unknown mux frame kind {0}")]
    UnknownKind(u8),
    #[error("the multiplexer speaks version {0}")]
    Version(u32),
    // keep-sorted end
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
