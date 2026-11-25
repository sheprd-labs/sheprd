//! `phone`: agents driving a paired phone's app through this computer's
//! remote-control daemon. The daemon's own types are the op and the reply
//! (`sheprd_remote_control::control::phone`): nothing pairs, signs, or
//! approves here, and a phone is driven only while its person allows it.
pub use sheprd_remote_control::control::phone::{Answer, Ask};

/// The daemon waits for the phone, within its own answer timeout.
pub const WAIT_SECONDS: u64 = 12;

#[cfg(test)]
mod tests;
