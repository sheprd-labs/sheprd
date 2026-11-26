//! The contract with Sheprd Agent, spoken by both `macos/sheprd-machine-agent/`
//! and `linux/sheprd-machine-agent/`: one request and its reply per connection,
//! each a 4-byte big-endian length and then JSON.

use super::{Error, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sheprd_control_protocol::protocol::tabs::{AgentCapabilities, MachineButton, MachineModifier};
use std::io::{Read, Write};

/// The vsock port the agent listens on.
pub(crate) const PORT: u32 = 7470;
/// The largest reply read: a screenshot of a large display, in base64.
pub(super) const MAX_REPLY: usize = 64 << 20;
/// Maximum accepted screenshot width or height in pixels.
const MAX_PNG_DIMENSION: u32 = 16_384;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum Request {
    Hello,
    Exec {
        argv: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        timeout_ms: u64,
    },
    Screenshot,
    Click {
        x: u32,
        y: u32,
        button: MachineButton,
        count: u8,
    },
    Type {
        text: String,
    },
    Key {
        key: String,
        modifiers: Vec<MachineModifier>,
    },
}

/// Why the agent did not do what it was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// macOS does not let it post input yet.
    Accessibility,
    /// macOS does not let it see the screen yet.
    ScreenRecording,
    /// The guest's graphical desktop session is not ready yet.
    NotReady,
    /// The guest does not support the requested operation or permission.
    Unsupported,
    Invalid,
    Failed,
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    error: Option<Refusal>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct Hello {
    pub(crate) version: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) scale: f64,
    #[serde(default)]
    pub(crate) accessibility: bool,
    #[serde(default)]
    pub(crate) screen_recording: bool,
    #[serde(default)]
    pub(crate) capabilities: Option<AgentCapabilities>,
}

impl Hello {
    /// Common capabilities reported by the agent, falling back to legacy macOS
    /// permission booleans when talking to an older macOS guest agent.
    pub(crate) fn effective_capabilities(&self) -> AgentCapabilities {
        self.capabilities.unwrap_or_else(|| {
            AgentCapabilities::from_macos_permissions(self.accessibility, self.screen_recording)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Exec {
    /// The exit code; none when a signal ended the command.
    #[serde(default)]
    pub(crate) status: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) truncated: bool,
    pub(crate) timed_out: bool,
}

#[derive(Deserialize)]
pub(super) struct Picture {
    pub(super) png: String,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// Nothing beyond `ok`.
#[derive(Debug, Deserialize)]
pub(super) struct Done {}

/// Sends `request` and reads its reply.
pub(super) fn exchange(mut stream: impl Read + Write, request: &Request) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(request).map_err(Error::Reply)?;
    let length = u32::try_from(body.len()).map_err(|_| Error::TooLarge(body.len()))?;
    stream.write_all(&length.to_be_bytes()).map_err(Error::Io)?;
    stream.write_all(&body).map_err(Error::Io)?;
    stream.flush().map_err(Error::Io)?;
    read_reply(stream)
}

pub(super) fn read_reply(mut stream: impl Read) -> Result<Vec<u8>> {
    let mut prefix = [0; 4];
    stream.read_exact(&mut prefix).map_err(Error::Io)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length > MAX_REPLY {
        return Err(Error::TooLarge(length));
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(Error::Io)?;
    Ok(body)
}

/// The reply in `body`, or the agent's refusal.
pub(super) fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T> {
    let envelope: Envelope = serde_json::from_slice(body).map_err(Error::Reply)?;
    if !envelope.ok {
        return Err(Error::Refused {
            refusal: envelope.error.unwrap_or(Refusal::Failed),
            message: envelope.message.unwrap_or_default(),
        });
    }
    serde_json::from_slice(body).map_err(Error::Reply)
}

/// A PNG's size in pixels, from its header, or `None` if `bytes` do not
/// start as a PNG does: the signature, then the IHDR chunk.
pub(super) fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    let header = bytes.get(..24)?;
    if &header[..8] != SIGNATURE || &header[12..16] != b"IHDR" {
        return None;
    }
    let number = |at: usize| Some(u32::from_be_bytes(header.get(at..at + 4)?.try_into().ok()?));
    let (width, height) = (number(16)?, number(20)?);
    if width == 0 || height == 0 || width > MAX_PNG_DIMENSION || height > MAX_PNG_DIMENSION {
        return None;
    }
    Some((width, height))
}

/// The last `limit` bytes of `text`, cut at a character, and whether any
/// were cut.
pub(super) fn tail(mut text: String, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let start = (text.len() - limit..text.len())
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(text.len());
    text.drain(..start);
    (text, true)
}
