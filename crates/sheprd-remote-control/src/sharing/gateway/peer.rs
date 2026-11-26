//! Native reference peer for the draft sharing protocol. It exercises the same
//! enrollment and read frames as a future browser, without an HTTP/phone client.
use crate::sharing::{
    Capability, Error, GrantId, Result, ShareId,
    gateway::{
        ADMISSION, Offer,
        io::{self, Tunnel},
        proof, signing_key,
    },
};
use p256::ecdsa::SigningKey;
use secrecy::ExposeSecret;
use serde::Deserialize;
use sheprd_chat_protocol::{
    Cursor,
    sharing::{MAX_RESPONSE_BYTES, Read, Update, VERSION},
};
use sheprd_relay::client;
use sheprd_tunnel::{Initiator, Keys, stream};
use tokio::io::join;

pub struct Peer {
    failed: bool,
    tunnel: Tunnel,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Admission {
    // keep-sorted start
    Ready { version: u32 },
    Registered { version: u32 },
    // keep-sorted end
}

impl Peer {
    /// The caller supplies a trusted relay connection and pinned invitation
    /// identity. There is no automatic URL resolution, credential forwarding,
    /// command replay or downgrade to the paired-device API.
    pub async fn connect(
        connection: &quinn::Connection,
        offer: &Offer,
        share: ShareId,
        grant: GrantId,
        capability: &Capability,
    ) -> Result<Self> {
        let keys = Keys::generate()?;
        let viewer = signing_key()?;
        let bootstrap =
            SigningKey::from_slice(offer.bootstrap.expose_secret()).map_err(|_| Error::Denied)?;
        let mut initial =
            tokio::time::timeout(ADMISSION, tunnel(connection, offer, &keys, &bootstrap))
                .await
                .map_err(|_| Error::Timeout)??;
        let enrollment = async {
            let bytes = proof::join(share, grant, capability, &viewer, &initial.0.hash)?;
            io::write(&mut initial.0.io, &bytes, 1024).await?;
            let registered: Admission =
                serde_json::from_slice(&io::read(&mut initial.0.io, 128).await?)?;
            match registered {
                Admission::Registered { version: 0 } => Ok(()),
                _ => Err(Error::Version),
            }
        };
        tokio::time::timeout(ADMISSION, enrollment)
            .await
            .map_err(|_| Error::Timeout)??;
        drop(initial);
        let mut tunnel = tokio::time::timeout(ADMISSION, tunnel(connection, offer, &keys, &viewer))
            .await
            .map_err(|_| Error::Timeout)??;
        tokio::time::timeout(ADMISSION, async {
            let bytes = proof::resume(share, grant, &viewer, &tunnel.0.hash)?;
            io::write(&mut tunnel.0.io, &bytes, 1024).await?;
            let ready: Admission = serde_json::from_slice(&io::read(&mut tunnel.0.io, 128).await?)?;
            match ready {
                Admission::Ready { version: 0 } => Ok(()),
                _ => Err(Error::Version),
            }
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(Self {
            failed: false,
            tunnel,
        })
    }

    pub async fn read(&mut self, cursor: Option<Cursor>) -> Result<Update> {
        if self.failed {
            self.tunnel.0.task.abort();
            return Err(Error::Denied);
        }
        // A cancelled future may have consumed part of a frame. It cannot be
        // retried on this connection; only a completed read clears the fence.
        self.failed = true;
        let result = tokio::time::timeout(ADMISSION, async {
            let request = serde_json::to_vec(&Read {
                version: VERSION,
                cursor,
            })?;
            io::write(&mut self.tunnel.0.io, &request, 1024).await?;
            let bytes = io::read(&mut self.tunnel.0.io, MAX_RESPONSE_BYTES).await?;
            Ok(serde_json::from_slice(&bytes)?)
        })
        .await
        .map_err(|_| Error::Timeout)
        .and_then(|result| result);
        if result.is_ok() {
            self.failed = false;
        } else {
            self.tunnel.0.task.abort();
        }
        result
    }
}

pub(super) async fn tunnel(
    connection: &quinn::Connection,
    offer: &Offer,
    keys: &Keys,
    dial: &SigningKey,
) -> Result<Tunnel> {
    let (send, recv) = client::dial(connection, offer.route, dial).await?;
    let (initiator, first) = Initiator::session(keys, &offer.identity, offer.route.as_bytes())?;
    Ok(Tunnel(
        stream::connect(join(recv, send), initiator, &first, offer.identity.x25519()).await?,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
