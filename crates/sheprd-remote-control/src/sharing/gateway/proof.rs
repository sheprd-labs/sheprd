//! The dial key proves possession inside the authenticated tunnel, over its
//! fresh handshake hash. A relay's incoming-key header is never authority.
use crate::sharing::{Capability, Error, GrantId, Result, ShareId};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde::Deserialize;
use sheprd_chat_protocol::sharing::admission::{self, JOIN, RESUME, message};
use sheprd_relay::route;

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

fn verifies(key: &[u8], proof: &[u8], message: &[u8]) -> Result<()> {
    if key.len() != 33 || proof.len() != 64 {
        return Err(Error::Denied);
    }
    route::prove(key, proof, message).map_err(|_| Error::Denied)?;
    Ok(())
}

/// Sign the shared wire format with the per-viewer dial key.
pub fn join(
    share: ShareId,
    grant: GrantId,
    capability: &Capability,
    key: &SigningKey,
    hash: &[u8; 32],
) -> Result<Vec<u8>> {
    Ok(admission::join(
        share,
        grant,
        capability.expose(),
        key.verifying_key().to_sec1_point(true).as_bytes(),
        |msg| signature(key, msg),
        hash,
    )?)
}

pub fn resume(
    share: ShareId,
    grant: GrantId,
    key: &SigningKey,
    hash: &[u8; 32],
) -> Result<Vec<u8>> {
    Ok(admission::resume(
        share,
        grant,
        |msg| signature(key, msg),
        hash,
    )?)
}

fn signature(key: &SigningKey, message: &[u8]) -> [u8; 64] {
    let signature: Signature = key.sign(message);
    signature.to_bytes().into()
}
