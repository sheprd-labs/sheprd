//! A client of T3 Code (`pingdotgg/t3code`): its threads, read and steered
//! over the same WebSocket protocol its own apps use, and shown as Sheprd
//! chats.
//!
//! T3 Code keeps running the agents and owning the threads; this crate only
//! reads and steers them (`docs/t3-plan.md`). The protocol has no published
//! specification, so only what Sheprd uses is typed, unknown kinds are
//! skipped, and the server version it was recorded against is [`TESTED`].
//! No GPUI and no async runtime.

pub mod connection;
pub mod discover;
mod error;
#[cfg(test)]
mod fixtures;
pub mod frame;
pub mod http;
pub mod launch;
pub mod shell;
pub mod steer;
pub mod thread;
pub mod token;
pub mod transcript;

pub use error::{Error, Result};

/// The newest T3 Code release this client's fixtures were recorded from
/// (`fixtures/README.md` names each file's).
pub const TESTED: &str = "0.1.9";
