//! Shared invitation fields and signed application frames for native and browser guests.
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShareId(pub Uuid);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrantId(pub Uuid);

pub const MAX_LINK_BYTES: usize = 8192;
pub const VIEWER_PATH: &str = "/share/";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: u32,
    pub relay: String,
    pub share: ShareId,
    pub grant: GrantId,
    pub expires_ms: u64,
    pub route: String,
    pub identity: String,
    pub bootstrap: String,
    pub capability: String,
}

pub const JOIN: &[u8] = b"sheprd-shared-session/join/v0\0";
pub const RESUME: &[u8] = b"sheprd-shared-session/resume/v0\0";

pub fn message(domain: &[u8], share: ShareId, grant: GrantId, hash: &[u8; 32]) -> Vec<u8> {
    [domain, share.0.as_bytes(), grant.0.as_bytes(), hash].concat()
}

/// Sensitive bytes, for the bootstrap tunnel only. Do not log this request.
pub fn join(
    share: ShareId,
    grant: GrantId,
    capability: &[u8; 32],
    public: &[u8],
    sign: impl FnOnce(&[u8]) -> [u8; 64],
    hash: &[u8; 32],
) -> serde_json::Result<Vec<u8>> {
    let signature = sign(&message(JOIN, share, grant, hash));
    serde_json::to_vec(&serde_json::json!({
        "version": 0, "share": share, "grant": grant,
        "capability": capability, "key": public,
        "proof": signature.as_slice(),
    }))
}

pub fn resume(
    share: ShareId,
    grant: GrantId,
    sign: impl FnOnce(&[u8]) -> [u8; 64],
    hash: &[u8; 32],
) -> serde_json::Result<Vec<u8>> {
    let signature = sign(&message(RESUME, share, grant, hash));
    serde_json::to_vec(&serde_json::json!({ "version": 0, "proof": signature.as_slice() }))
}
