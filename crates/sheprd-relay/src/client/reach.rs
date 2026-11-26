//! Reaching a relay by its endpoint: its `/v1/info` over HTTPS for the
//! QUIC port and certificate pin, every address its name resolves to, and
//! a QUIC connection to the first that answers. Shared by everything that
//! listens on or dials a relay, so each tries addresses the same way.

use super::{Error, Result, connect};
use crate::{
    Endpoint,
    endpoint::{Info, QuicInfo},
    pin::Pin,
};
use quinn::Connection;
use std::{future::Future, net::SocketAddr, time::Duration};

/// How long reading `/v1/info` may take.
const INFO_TIMEOUT: Duration = Duration::from_secs(10);
/// The most of `/v1/info` read; it is a few hundred bytes.
const INFO_LIMIT: u64 = 16 * 1024;
/// How long one of a relay's addresses may take to answer before the next
/// is tried. The last is given QUIC's own handshake time.
const ATTEMPT: Duration = Duration::from_secs(5);
/// How many of a relay's addresses are tried per connection.
const MAX_ADDRESSES: usize = 8;

/// Connects to the relay at `endpoint` over QUIC, at the first of its
/// addresses that answers, trusting the certificate `/v1/info` names.
pub async fn reach(endpoint: &Endpoint) -> Result<Connection> {
    reach_pinned(endpoint, None).await
}

/// [`reach`], but when `expected` holds a pin, such as the one Sheprd's
/// signed relay list carries, only a certificate with that pin is
/// trusted: a `/v1/info` naming another, which a TLS-inspecting proxy or a
/// mis-issued certificate could forge, fails with [`Error::PinMismatch`]
/// and nothing is dialed.
pub async fn reach_pinned(endpoint: &Endpoint, expected: Option<Pin>) -> Result<Connection> {
    let (addresses, reported) = quic(endpoint).await?;
    let pin = trusted(reported, expected)?;
    first_reachable(&addresses, ATTEMPT, |address| connect(address, pin)).await
}

/// The pin to dial with: the one expected when there is one, which the
/// relay must also report.
fn trusted(reported: Pin, expected: Option<Pin>) -> Result<Pin> {
    match expected {
        Some(expected) if expected != reported => Err(Error::PinMismatch),
        _ => Ok(reported),
    }
}

/// Connects to the first of `addresses` that answers, in the resolver's
/// order: a relay with IPv6 and IPv4 records is still reached from a
/// network with a route to only one of them. An address that fails, or
/// takes longer than `attempt` while another is left, gives way to the
/// next; the last one's failure is the one reported.
pub async fn first_reachable<C, F>(
    addresses: &[SocketAddr],
    attempt: Duration,
    mut connect: impl FnMut(SocketAddr) -> F,
) -> Result<C>
where
    F: Future<Output = Result<C>>,
{
    let (last, earlier) = addresses
        .split_last()
        .ok_or_else(|| Error::Resolve(std::io::Error::other("no address")))?;
    for address in earlier {
        if let Ok(Ok(connection)) = tokio::time::timeout(attempt, connect(*address)).await {
            return Ok(connection);
        }
    }
    connect(*last).await
}

/// Where the relay's QUIC listens (every address its name resolves to, up
/// to [`MAX_ADDRESSES`]), and its certificate's pin, from its `/v1/info`.
async fn quic(endpoint: &Endpoint) -> Result<(Vec<SocketAddr>, Pin)> {
    let url = endpoint.info().to_string();
    let info = tokio::task::spawn_blocking(move || info(&url))
        .await
        .map_err(|_| Error::InfoFormat)??;
    let quic = info.quic.ok_or(Error::NoQuic)?;
    let pin = pin(&quic)?;
    let host = endpoint.hostname();
    let addresses = tokio::net::lookup_host((host.as_str(), quic.port))
        .await
        .map_err(Error::Resolve)?
        .take(MAX_ADDRESSES)
        .collect();
    Ok((addresses, pin))
}

/// Reads `/v1/info` at `url`. Blocking.
fn info(url: &str) -> Result<Info> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(INFO_TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(Box::new)?;
    let status = response.status();
    let headers = response.headers();
    if status.as_u16() == 403
        && crate::endpoint::is_ban(|name| headers.get(name).and_then(|value| value.to_str().ok()))
    {
        let header = |name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
        };
        return Err(Error::Banned(crate::ban::Ban::from_parts(
            header(crate::ban::REASON_HEADER),
            header(crate::ban::SECONDS_HEADER),
        )));
    }
    if !status.is_success() {
        return Err(Error::Info(Box::new(ureq::Error::StatusCode(
            status.as_u16(),
        ))));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(INFO_LIMIT)
        .read_to_vec()
        .map_err(Box::new)?;
    serde_json::from_slice(&body).map_err(|_| Error::InfoFormat)
}

fn pin(quic: &QuicInfo) -> Result<Pin> {
    crate::endpoint::decode_pin(&quic.pin).ok_or(Error::InfoFormat)
}

#[cfg(test)]
mod tests;
