use std::time::Duration;

use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{Cursor, EndpointId, EntryId, Transcript};
use uuid::Uuid;

use crate::sharing::{Error, Result};

pub const MAX_SHARES: usize = 8;
pub const MAX_GRANTS: usize = 16;
pub const MAX_VIEWERS: usize = 16;
pub const MAX_LIFETIME: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShareId(pub Uuid);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrantId(pub Uuid);

/// Supplied by the host, never by a recipient. Both clocks must advance.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub unix_ms: u64,
    pub elapsed: Duration,
}

/// Explicit owner choice. A new share defaults to excluding existing entries,
/// including any in-flight tool whose content changes after sharing starts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum History {
    All,
    #[default]
    FromNow,
}

/// Additional content beyond user and agent text. No file or image access is
/// implied. Tool text can contain sensitive data, so it is off by default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub tools: bool,
}

/// A host-observed attach snapshot. The endpoint, epoch and agent session ID
/// together prevent a reused pane or socket from inheriting a grant.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    pub endpoint: &'a EndpointId,
    pub cursor: Cursor,
    pub transcript: &'a Transcript,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    endpoint: String,
    epoch: Uuid,
    session: String,
}

impl Binding {
    pub(crate) fn capture(source: Source<'_>) -> Result<Self> {
        let session = source.transcript.session.as_ref().ok_or(Error::Source)?;
        if session.id.is_empty() || session.id.len() > 512 || source.cursor.epoch.is_nil() {
            return Err(Error::Source);
        }
        Ok(Self {
            endpoint: source.endpoint.as_str().to_owned(),
            epoch: source.cursor.epoch,
            session: session.id.clone(),
        })
    }

    pub(crate) fn matches(&self, source: Source<'_>) -> bool {
        self.endpoint == source.endpoint.as_str()
            && self.epoch == source.cursor.epoch
            && source
                .transcript
                .session
                .as_ref()
                .is_some_and(|s| s.id == self.session)
    }

    pub(crate) fn valid(&self) -> bool {
        self.endpoint.parse::<EndpointId>().is_ok()
            && !self.epoch.is_nil()
            && !self.session.is_empty()
            && self.session.len() <= 512
    }
}

/// Kept only while returning an invitation to its owner. Never serialized or
/// logged implicitly; the future invitation encoder must explicitly expose it.
#[derive(Debug)]
pub struct Capability(SecretBox<[u8; 32]>);

impl Capability {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    pub(crate) fn generate() -> Result<Self> {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(Error::Random)?;
        Ok(Self::from_bytes(bytes))
    }

    pub(crate) fn digest(&self) -> [u8; 32] {
        crate::auth::sha256(&[b"sheprd-share-capability-v0\0", self.expose()])
    }
}

#[derive(Debug)]
pub struct Invitation {
    pub share: ShareId,
    pub grant: GrantId,
    pub capability: Capability,
    pub expires_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Grant {
    pub(crate) id: GrantId,
    pub(crate) digest: [u8; 32],
    pub(crate) expires_ms: u64,
    pub(crate) revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Share {
    pub(crate) id: ShareId,
    pub(crate) binding: Binding,
    pub(crate) boundary: Option<EntryId>,
    pub(crate) content: Content,
    pub(crate) created_ms: u64,
    pub(crate) expires_ms: u64,
    pub(crate) grants: Vec<Grant>,
    pub(crate) source_seq: u64,
    pub(crate) stopped: bool,
}
