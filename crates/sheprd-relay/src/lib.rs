//! A relay that joins two peers with no direct path to each other: a
//! **listener**, such as a desktop app, and a **dialer**, such as a phone.
//! `PROTOCOL.md` specifies the wire format for implementers; the vectors in
//! `vectors/` give its exact bytes.
//!
//! The relay is a blind pipe. Peers run their own end-to-end encrypted
//! handshake inside the stream it carries, so taking the relay over yields
//! ciphertext, timing, and sizes, never content:
//!
//! - A listener **listens** on a [`RouteId`], the hash of a P-256 public
//!   key it proves it holds. Nothing else identifies it.
//! - A dialer **dials** that route with a key the listener named. The
//!   relay tells the listener, which **accepts** with a fresh socket or
//!   stream, and the two are spliced.
//! - Every resource is bounded: connections in total and per address,
//!   listeners, sessions, frame sizes, and bandwidth per session and in
//!   total ([`Limits`]). Nothing is stored and nothing survives a restart.
//! - Logs carry no addresses, routes, or payloads. The dashboard shows only
//!   the start of a route.
//!
//! Always built, for any client:
//!
//! - [`ban`]: why and how long a relay banned an address, as it tells
//!   the client.
//! - [`endpoint`]: a relay's address and the URLs built from it.
//! - [`route`]: route and session ids, proofs, and pairing dial keys.
//! - [`socket`]: the WebSocket protocol's messages and close codes.
//! - [`wire`]: the QUIC protocol's binary requests and statuses.
//! - [`framing`]: the optional message layer inside a session.
//!
//! Behind features:
//!
//! - `client` ([`client`]): a listener and dialer over QUIC.
//! - `client-ws` ([`client_ws`]): a listener and dialer over WebSockets.
//! - `testing` ([`testing`]): a WebSocket relay on a loopback port, to run
//!   inside other projects' tests.
//! - `websocket-server`: the relay over WebSockets: [`hub`] (who listens,
//!   who waits, what each address holds), [`splice`] (carrying bytes),
//!   [`guard`] (banning addresses that misbehave), [`limits`], [`bucket`]
//!   (rate limits and throttles), [`metrics`], and [`web`] (the HTTP
//!   routes).
//! - `quic-server`: [`quic`], the QUIC transport beside it.
//! - `ops` ([`ops`]): rate limits, secrets, Prometheus text, and a
//!   dashboard kit, for this relay and other services; [`secret`].
//! - `dashboard` ([`dashboard`]): the operator's read-only page.
//! - `server` (default): all of the above as deployed, with [`options`]
//!   (flags and `RELAY_*` variables) and the `sheprd-relay` binary.

pub mod ban;
#[cfg(feature = "websocket-server")]
mod broker;
#[cfg(feature = "websocket-server")]
pub mod bucket;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "client-ws")]
pub mod client_ws;
mod commit;
#[cfg(feature = "dashboard")]
pub mod dashboard;
pub mod endpoint;
mod error;
pub mod framing;
#[cfg(feature = "websocket-server")]
pub mod guard;
#[cfg(feature = "websocket-server")]
pub mod hub;
#[cfg(feature = "websocket-server")]
pub mod limits;
#[cfg(feature = "websocket-server")]
pub mod metrics;
#[cfg(feature = "ops")]
pub mod ops;
#[cfg(feature = "server")]
pub mod options;
#[cfg(any(feature = "quic-server", feature = "client"))]
pub mod pin;
#[cfg(feature = "quic-server")]
pub mod quic;
pub mod route;
#[cfg(feature = "ops")]
pub use ops::secret;
#[cfg(feature = "server")]
mod serve;
pub mod socket;
#[cfg(feature = "websocket-server")]
pub mod splice;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(feature = "server")]
mod viewer;
#[cfg(feature = "websocket-server")]
pub mod web;
pub mod wire;

pub use commit::commit;
pub use endpoint::Endpoint;
pub use error::{Error, Result};
#[cfg(feature = "websocket-server")]
pub use limits::Limits;
#[cfg(feature = "server")]
pub use options::{Options, USAGE};
pub use route::{RouteId, SessionId};
#[cfg(feature = "server")]
pub use serve::{app, bind_quic, serve};

#[cfg(test)]
mod tests;
