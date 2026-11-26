//! The dial key proves possession inside the authenticated tunnel, over its
//! fresh handshake hash. A relay's incoming-key header is never authority.
use crate::sharing::{Capability, Error, GrantId, Result, ShareId};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde::Deserialize;
use sheprd_relay::{client, route};

const JOIN: &[u8] = b"sheprd-shared-session/join/v0\0";
const RESUME: &[u8] = b"sheprd-shared-session/resume/v0\0";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Join {
    version: u32,
    share: ShareId,
    pub(super) grant: GrantId,
    capability: [u8; 32],
    pub(super) key: Vec<u8>,
    proof: Vec<u8>,
}

impl Join {
    pub(super) fn verify(&self, share: ShareId, hash: &[u8; 32]) -> Result<Capability> {
        if self.version != 0 || self.share != share {
            return Err(Error::Denied);
        }
        verifies(
            &self.key,
            &self.proof,
            &message(JOIN, share, self.grant, hash),
        )?;
        Ok(Capability::from_bytes(self.capability))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Resume {
    version: u32,
    proof: Vec<u8>,
}

impl Resume {
    pub(super) fn verify(
        &self,
        key: &[u8],
        share: ShareId,
        grant: GrantId,
        hash: &[u8; 32],
    ) -> Result<()> {
        if self.version != 0 {
            return Err(Error::Version);
        }
        verifies(key, &self.proof, &message(RESUME, share, grant, hash))
    }
}

fn message(domain: &[u8], share: ShareId, grant: GrantId, hash: &[u8; 32]) -> Vec<u8> {
    [domain, share.0.as_bytes(), grant.0.as_bytes(), hash].concat()
}

fn verifies(key: &[u8], proof: &[u8], message: &[u8]) -> Result<()> {
    if key.len() != 33 || proof.len() != 64 {
        return Err(Error::Denied);
    }
    route::prove(key, proof, message).map_err(|_| Error::Denied)?;
    Ok(())
}

/// Sensitive bytes, for the bootstrap tunnel only. Do not log this request.
pub fn join(
    share: ShareId,
    grant: GrantId,
    capability: &Capability,
    key: &SigningKey,
    hash: &[u8; 32],
) -> Result<Vec<u8>> {
    let signature: Signature = key.sign(&message(JOIN, share, grant, hash));
    Ok(serde_json::to_vec(&serde_json::json!({
        "version": 0, "share": share, "grant": grant,
        "capability": capability.expose(), "key": client::public(key).as_slice(),
        "proof": signature.to_bytes().as_slice(),
    }))?)
}

pub fn resume(
    share: ShareId,
    grant: GrantId,
    key: &SigningKey,
    hash: &[u8; 32],
) -> Result<Vec<u8>> {
    let signature: Signature = key.sign(&message(RESUME, share, grant, hash));
    Ok(serde_json::to_vec(
        &serde_json::json!({ "version": 0, "proof": signature.to_bytes().as_slice() }),
    )?)
}
