//! The multiplexer's frames: a kind byte, the stream id and the payload
//! length as little-endian `u32`s, then the payload. Decoding is strict:
//! an unknown kind, a wrong payload size, or a stream id where none belongs
//! is an error, never skipped.

use std::io::{self, Read, Write};

use super::{Error, MAGIC, MAX_DATA, Result, VERSION};
use crate::EndpointId;

/// A stream's number, chosen by the client: nonzero and rising, never
/// reused within one connection. Zero names the connection itself.
pub type StreamId = u32;

const HELLO: u8 = 1;
const OPEN: u8 = 2;
const DATA: u8 = 3;
const CREDIT: u8 = 4;
const CLOSE: u8 = 5;
const HEADER: usize = 9;
const HELLO_LEN: usize = MAGIC.len() + 8;
/// An endpoint id's length on the wire: its 32 hexadecimal digits.
const ENDPOINT_LEN: usize = 32;

/// The host's greeting, its first frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hello {
    pub version: u32,
    /// Streams the host carries at once.
    pub max_streams: u32,
}

/// Why a side closed a stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReason {
    /// The connection behind it ended, or its reader is done.
    Ended,
    /// The host could not reach the chat endpoint.
    Unreachable,
    /// The host already carries as many streams as it may.
    Limit,
}

impl CloseReason {
    fn byte(self) -> u8 {
        match self {
            Self::Ended => 0,
            Self::Unreachable => 1,
            Self::Limit => 2,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Ended,
            1 => Self::Unreachable,
            2 => Self::Limit,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Frame {
    Hello(Hello),
    /// Client to host: connect `stream` to the chat at `endpoint`.
    Open {
        stream: StreamId,
        endpoint: EndpointId,
    },
    /// At most [`MAX_DATA`] bytes, never empty.
    Data {
        stream: StreamId,
        bytes: Vec<u8>,
    },
    /// The receiver consumed `bytes` more, so the sender may send them.
    Credit {
        stream: StreamId,
        bytes: u32,
    },
    /// The sender sends no more and reads no more on `stream`.
    Close {
        stream: StreamId,
        reason: CloseReason,
    },
}

impl Frame {
    /// The host's greeting for this version.
    pub fn hello(max_streams: usize) -> Self {
        Self::Hello(Hello {
            version: VERSION,
            max_streams: u32::try_from(max_streams).unwrap_or(u32::MAX),
        })
    }

    fn kind(&self) -> u8 {
        match self {
            Self::Hello(_) => HELLO,
            Self::Open { .. } => OPEN,
            Self::Data { .. } => DATA,
            Self::Credit { .. } => CREDIT,
            Self::Close { .. } => CLOSE,
        }
    }

    fn stream(&self) -> StreamId {
        match self {
            Self::Hello(_) => 0,
            Self::Open { stream, .. }
            | Self::Data { stream, .. }
            | Self::Credit { stream, .. }
            | Self::Close { stream, .. } => *stream,
        }
    }

    /// Appends the frame's bytes to `out`. Data longer than [`MAX_DATA`] is
    /// an error, so no peer is sent what it must refuse.
    pub fn encode(&self, out: &mut Vec<u8>) -> Result<()> {
        let start = out.len();
        out.push(self.kind());
        out.extend_from_slice(&self.stream().to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        match self {
            Self::Hello(hello) => {
                out.extend_from_slice(&MAGIC);
                out.extend_from_slice(&hello.version.to_le_bytes());
                out.extend_from_slice(&hello.max_streams.to_le_bytes());
            }
            Self::Open { endpoint, .. } => out.extend_from_slice(endpoint.as_str().as_bytes()),
            Self::Data { bytes, .. } => {
                if bytes.is_empty() || bytes.len() > MAX_DATA {
                    out.truncate(start);
                    return Err(Error::TooLarge(bytes.len()));
                }
                out.extend_from_slice(bytes);
            }
            Self::Credit { bytes, .. } => out.extend_from_slice(&bytes.to_le_bytes()),
            Self::Close { reason, .. } => out.push(reason.byte()),
        }
        let len = out.len() - start - HEADER;
        let prefix = u32::try_from(len).map_err(|_| Error::TooLarge(len))?;
        out[start + 5..start + HEADER].copy_from_slice(&prefix.to_le_bytes());
        Ok(())
    }

    pub fn write_to(&self, writer: &mut impl Write) -> Result<()> {
        let mut bytes = Vec::new();
        self.encode(&mut bytes)?;
        Ok(writer.write_all(&bytes)?)
    }

    /// Reads one frame, or `None` when the peer closed between frames. A
    /// stream that ends inside a frame is an error.
    pub fn read_from(reader: &mut impl Read) -> Result<Option<Self>> {
        let mut header = [0; HEADER];
        let mut filled = 0;
        while filled < HEADER {
            match reader.read(&mut header[filled..]) {
                Ok(0) if filled == 0 => return Ok(None),
                Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        let kind = header[0];
        let stream = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
        let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        if len > MAX_DATA {
            return Err(Error::TooLarge(len));
        }
        let expected = match kind {
            HELLO => Some(HELLO_LEN),
            OPEN => Some(ENDPOINT_LEN),
            DATA => None,
            CREDIT => Some(4),
            CLOSE => Some(1),
            _ => return Err(Error::UnknownKind(kind)),
        };
        let malformed = Error::Malformed(kind);
        if expected.is_some_and(|expected| expected != len)
            || (kind == DATA && len == 0)
            || ((kind == HELLO) != (stream == 0))
        {
            return Err(malformed);
        }
        let mut payload = vec![0; len];
        reader.read_exact(&mut payload)?;
        let word = |at: usize| {
            payload
                .get(at..at + 4)
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .map(u32::from_le_bytes)
        };
        Ok(Some(match kind {
            HELLO => {
                if payload.get(..MAGIC.len()) != Some(&MAGIC[..]) {
                    return Err(Error::NotMultiplexer);
                }
                let (Some(version), Some(max_streams)) = (word(MAGIC.len()), word(MAGIC.len() + 4))
                else {
                    return Err(malformed);
                };
                Self::Hello(Hello {
                    version,
                    max_streams,
                })
            }
            OPEN => {
                let endpoint = std::str::from_utf8(&payload)
                    .ok()
                    .and_then(|text| text.parse().ok())
                    .ok_or(malformed)?;
                Self::Open { stream, endpoint }
            }
            DATA => Self::Data {
                stream,
                bytes: payload,
            },
            CREDIT => match word(0) {
                Some(bytes) if bytes > 0 => Self::Credit { stream, bytes },
                _ => return Err(malformed),
            },
            _ => match payload.first().copied().and_then(CloseReason::from_byte) {
                Some(reason) => Self::Close { stream, reason },
                None => return Err(malformed),
            },
        }))
    }
}
