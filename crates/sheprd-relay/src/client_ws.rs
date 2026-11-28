//! Reaching a relay over its WebSocket API, as a listener or a dialer: the
//! client side of [`crate::socket`], for apps that cannot or need not use
//! QUIC. Tokio only; `wss` checks the relay's certificate as the platform
//! does.
//!
//! - [`dial`] reaches the listener on a route, or says why the relay
//!   refused ([`Error::Refused`] with its [`Status`]).
//! - [`Listener`] stays on its key's route: it reconnects with capped
//!   backoff, names its dial keys and pairing keys again after each
//!   reconnect, reports where it stands ([`ListenerStatus`]), and stops
//!   for good when another listener takes the route (close code 4409).
//! - A [`Session`] is a byte stream (`AsyncRead` + `AsyncWrite`) that pings
//!   the relay so it does not idle out, and [`Session::framed`] reads and
//!   writes whole messages ([`crate::framing`]).
//!
//! What crosses a session is the peers' own end-to-end protocol; the relay
//! only carries it.
//!
//! ```no_run
//! # async fn example() -> sheprd_relay::client_ws::Result<()> {
//! use sheprd_relay::{Endpoint, RouteId, client_ws};
//! # let route = RouteId::from_bytes([0; 32]);
//! let endpoint = Endpoint::parse("relay.example.org").map_err(|_| client_ws::Error::Host)?;
//! let key = p256::ecdsa::SigningKey::from_slice(&[7; 32]).map_err(|_| client_ws::Error::Malformed)?;
//! let session = client_ws::dial(&endpoint, &route, &key).await?;
//! let mut messages = session.framed(4096);
//! messages.send(b"hello, acme").await?;
//! # Ok(())
//! # }
//! ```

mod connect;
mod error;
mod listener;
mod session;

pub use connect::{Stream, public};
pub use error::{Error, Result};
pub use listener::{
    Applied, Backoff, Incoming, Listener, ListenerConfig, ListenerStatus, Unreachable,
};
pub use session::{Framed, Session};

use crate::{
    Endpoint, RouteId,
    socket::{ToPeer, dial_signed},
    wire::Status,
};
use p256::ecdsa::SigningKey;
use std::time::Duration;

/// How sessions and handshakes behave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// How long opening a socket and proving a key may take. A dial's
    /// includes the relay waiting for the listener to accept, 10 seconds
    /// on a default relay.
    pub handshake_timeout: Duration,
    /// How often a session pings the relay while it is read, so a quiet
    /// session does not idle out; `None` sends no pings.
    pub keepalive: Option<Duration>,
    /// The largest binary frame a session writes. A relay refuses frames
    /// over its `max-frame`, 64 KiB by default and never under 1 KiB.
    pub max_frame: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(30),
            keepalive: Some(Duration::from_secs(30)),
            max_frame: 16 * 1024,
        }
    }
}

/// Dials `route` on `endpoint` as `key`, with the default [`Config`].
pub async fn dial(endpoint: &Endpoint, route: &RouteId, key: &SigningKey) -> Result<Session> {
    dial_with(endpoint, route, key, &Config::default()).await
}

/// Dials `route` on `endpoint` as `key`. A key the listener does not name,
/// like a route no one listens on, is refused as [`Status::Offline`].
pub async fn dial_with(
    endpoint: &Endpoint,
    route: &RouteId,
    key: &SigningKey,
    config: &Config,
) -> Result<Session> {
    let mut socket = connect::open(&endpoint.dial(route), config).await?;
    let name = endpoint.proof_name();
    let answered = async {
        connect::prove(&mut socket, key, None, |nonce| {
            dial_signed(&name, nonce, route)
        })
        .await?;
        match connect::control(&mut socket).await? {
            ToPeer::Status {
                status: Status::Ok, ..
            } => Ok(()),
            ToPeer::Status { status, .. } => Err(Error::Refused(status)),
            _ => Err(Error::Malformed),
        }
    };
    tokio::time::timeout(config.handshake_timeout, answered)
        .await
        .map_err(|_| Error::Timeout)??;
    Ok(Session::new(socket, config))
}

#[cfg(all(test, feature = "testing"))]
mod tests;
