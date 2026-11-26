//! Anonymous read-only invitation links for a single configured relay.
//! Parsing is offline. The caller supplies the relay it already trusts.

use crate::sharing::{Capability, Error, GrantId, Invitation, Result, ShareId, gateway::Offer};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use secrecy::{ExposeSecret, SecretBox, SecretString};
use serde::{Deserialize, Serialize};
use sheprd_relay::{Endpoint, RouteId};
use sheprd_tunnel::Identity;

const MAX_LINK_BYTES: usize = 8192;
const VIEWER_PATH: &str = "/share/";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    relay: String,
    share: ShareId,
    grant: GrantId,
    expires_ms: u64,
    route: String,
    identity: String,
    bootstrap: String,
    capability: String,
}

/// Make a copyable link after the relay acknowledges the grant's offer.
pub fn encode(relay: &Endpoint, invite: &Invitation, offer: &Offer) -> Result<SecretString> {
    if !relay.is_secure() {
        return Err(Error::InvalidInvitation);
    }
    let envelope = Envelope {
        version: 0,
        relay: relay.to_string(),
        share: invite.share,
        grant: invite.grant,
        expires_ms: invite.expires_ms,
        route: offer.route.to_string(),
        identity: URL_SAFE_NO_PAD.encode(offer.identity.to_bytes()),
        bootstrap: URL_SAFE_NO_PAD.encode(offer.bootstrap.expose_secret()),
        capability: URL_SAFE_NO_PAD.encode(invite.capability.expose()),
    };
    let fragment = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope)?);
    let link = format!("{relay}{VIEWER_PATH}#{fragment}");
    if link.len() > MAX_LINK_BYTES {
        return Err(Error::Limit);
    }
    Ok(link.into())
}

/// Resolve the credentials against an independently selected HTTPS relay. The
/// host enforces the actual grant expiry again during admission and reads.
pub fn decode(link: &str, trusted: &Endpoint, now_ms: u64) -> Result<(Invitation, Offer)> {
    if link.len() > MAX_LINK_BYTES {
        return Err(Error::Limit);
    }
    let (landing, fragment) = link.split_once('#').ok_or(Error::InvalidInvitation)?;
    if !trusted.is_secure() || landing != format!("{trusted}{VIEWER_PATH}") {
        return Err(Error::InvalidInvitation);
    }
    let json = URL_SAFE_NO_PAD
        .decode(fragment)
        .map_err(|_| Error::InvalidInvitation)?;
    // Serde diagnostics for untrusted fields can contain attacker-supplied text.
    let envelope: Envelope = serde_json::from_slice(&json).map_err(|_| Error::InvalidInvitation)?;
    if envelope.version != 0 {
        return Err(Error::Version);
    }
    if envelope.relay != trusted.to_string()
        || envelope.share.0.is_nil()
        || envelope.grant.0.is_nil()
    {
        return Err(Error::InvalidInvitation);
    }
    if now_ms >= envelope.expires_ms {
        return Err(Error::Denied);
    }
    let route = RouteId::parse(&envelope.route).ok_or(Error::InvalidInvitation)?;
    let identity = URL_SAFE_NO_PAD
        .decode(envelope.identity)
        .map_err(|_| Error::InvalidInvitation)?;
    let bootstrap = credential(&envelope.bootstrap)?;
    SigningKey::from_slice(&bootstrap).map_err(|_| Error::InvalidInvitation)?;
    Ok((
        Invitation {
            share: envelope.share,
            grant: envelope.grant,
            expires_ms: envelope.expires_ms,
            capability: Capability::from_bytes(credential(&envelope.capability)?),
        },
        Offer {
            route,
            identity: Identity::from_bytes(&identity).map_err(|_| Error::InvalidInvitation)?,
            bootstrap: SecretBox::new(Box::new(bootstrap)),
        },
    ))
}

fn credential(encoded: &str) -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    if URL_SAFE_NO_PAD.decode_slice(encoded, &mut bytes).ok() != Some(bytes.len()) {
        return Err(Error::InvalidInvitation);
    }
    Ok(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
