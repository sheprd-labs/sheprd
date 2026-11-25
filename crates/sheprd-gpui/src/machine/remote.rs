//! Virtual Macs on other computers (`docs/remote-machines-plan.md`). The
//! Sheprd running a machine is the only one that can: its picture, its
//! vsock, and its claims live in that process. So another computer borrows
//! one by running `sheprd machine lend` there over its host connection,
//! one request per run, and this module holds both ends of that: the
//! frames the lending side makes, and the borrowing side's runs.

mod error;
pub(crate) mod frame;
pub(crate) mod link;
pub(crate) mod watch;

pub use error::Error;
