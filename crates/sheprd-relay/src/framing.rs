//! An optional message layer for sessions: each message is its length, a
//! big-endian `u16`, then that many bytes. The relay never reads it; a
//! session is bytes, and peers that want messages agree on this among
//! themselves. WebSocket frames and QUIC chunks may split or join messages
//! anywhere, so a reader keeps what it has until a message is whole.
//!
//! [`encode`] writes one message; [`Deframer`] reads them back from bytes
//! as they arrive, holding at most a bounded buffer.

/// The bytes ahead of each message.
pub const PREFIX: usize = 2;
/// The longest message the prefix can describe.
pub const MAX_MESSAGE: usize = u16::MAX as usize;

/// Why bytes cannot be framed or read as messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a framed message of {length} bytes is over this reader's {cap}-byte limit")]
    OverCap { cap: usize, length: usize },
    #[error("a framed message holds at most {MAX_MESSAGE} bytes, not {length}")]
    TooLong { length: usize },
    // keep-sorted end
}

/// `message`, framed: its length, then the message.
pub fn encode(message: &[u8]) -> Result<Vec<u8>, Error> {
    let length = u16::try_from(message.len()).map_err(|_| Error::TooLong {
        length: message.len(),
    })?;
    let mut framed = Vec::with_capacity(PREFIX + message.len());
    framed.extend_from_slice(&length.to_be_bytes());
    framed.extend_from_slice(message);
    Ok(framed)
}

/// Reads framed messages from bytes pushed in any pieces. It refuses a
/// message longer than its cap as soon as the prefix says so, before
/// waiting for its body, so a reader that pushes bounded chunks and drains
/// [`Deframer::next_message`] holds at most a message, its prefix, and one chunk.
///
/// It does no I/O, so it is cancel-safe by construction: bytes pushed stay
/// pushed, whatever happens to the task that read them.
#[derive(Clone, Debug)]
pub struct Deframer {
    buffer: Vec<u8>,
    cap: usize,
    /// Where the next message starts in `buffer`: bytes before it were
    /// given out already, and are dropped when more arrive, so draining a
    /// chunk of many short messages copies it once, not once per message.
    start: usize,
}

impl Default for Deframer {
    fn default() -> Self {
        Self::new(MAX_MESSAGE)
    }
}

impl Deframer {
    /// A reader for messages of at most `cap` bytes, itself at most
    /// [`MAX_MESSAGE`].
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            buffer: Vec::new(),
            cap: cap.min(MAX_MESSAGE),
            start: 0,
        }
    }

    /// The longest message this reader takes.
    #[must_use]
    pub const fn cap(&self) -> usize {
        self.cap
    }

    /// Bytes held toward the next message.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len() - self.start
    }

    /// How many more bytes the next message needs before
    /// [`Deframer::next_message`] can give it: at least one when it is not whole.
    #[must_use]
    pub fn wanted(&self) -> usize {
        match self.length() {
            Some(length) => (PREFIX + length).saturating_sub(self.buffered()).max(1),
            None => PREFIX - self.buffered(),
        }
    }

    /// Adds bytes that arrived, then gives the first whole message, if one
    /// is. Call [`Deframer::next_message`] until it gives `None` to drain the rest.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        self.buffer.drain(..self.start);
        self.start = 0;
        self.buffer.extend_from_slice(bytes);
        self.next_message()
    }

    /// The next whole message from what was pushed, if one is.
    pub fn next_message(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let Some(length) = self.length() else {
            return Ok(None);
        };
        if length > self.cap {
            return Err(Error::OverCap {
                cap: self.cap,
                length,
            });
        }
        if self.buffered() < PREFIX + length {
            return Ok(None);
        }
        let body = self.start + PREFIX;
        let message = self.buffer[body..body + length].to_vec();
        self.start = body + length;
        if self.start == self.buffer.len() {
            self.buffer.clear();
            self.start = 0;
        }
        Ok(Some(message))
    }

    /// The next message's length, once its prefix is in.
    fn length(&self) -> Option<usize> {
        match &self.buffer[self.start..] {
            [high, low, ..] => Some(usize::from(u16::from_be_bytes([*high, *low]))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
