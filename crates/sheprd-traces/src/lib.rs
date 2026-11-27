//! Traces: an index of the agent sessions a host keeps (Claude Code, Codex,
//! Pi, omp), so Sheprd can list, chart, and search them across machines.
//!
//! The index is one private SQLite file in the state directory
//! (`location`). [`index`] scans the agents' native session files (`scan`),
//! reads only bytes added since the last scan, projects each record into
//! bounded, redacted [`Event`]s (`parse`), and keeps per-session totals
//! (`store`). [`watch`] keeps it current as agents write, reading only the
//! files the OS reports changed. Session files are data: nothing in them is run or followed.
//!
//! Every call blocks on disk I/O; the app never runs one on the UI thread.
//! No GPUI, no runtime.

#![cfg_attr(test, allow(clippy::disallowed_methods))]

mod error;
mod host;
mod location;
mod model;
mod parse;
pub mod redact;
mod roots;
mod scan;
mod store;
pub mod sync;
#[cfg(test)]
mod tests;
mod watch;

pub use error::{Error, Result};
pub use host::Host;
pub use location::{FILE, path};
pub use model::{ActiveAgent, Activity, Event, EventKind, Session, Visibility};
pub use roots::{configured_roots, default_roots};
pub use scan::{Report, Root, index, index_paths};
pub use store::{MAX_LIMIT, Overlap, OverlapCache, OverlapPoint, SCHEMA_VERSION, Store};
pub use watch::{RESCAN, SETTLE, Tick, watch};
