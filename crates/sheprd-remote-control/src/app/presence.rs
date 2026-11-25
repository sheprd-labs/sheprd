//! `presence`: Sheprd on another computer, the one a paired phone is near
//! over Bluetooth, marks that phone's device here near or away over SSH,
//! so this host's daemon holds the phone's alerts while its owner is at
//! that computer, as the computer's own daemon does (`push::presence`).
//! Sheprd renews a near mark every 30 s; the daemon lets one lapse after
//! two minutes, so a computer that went away never holds alerts for long.

use crate::control::{self, DeviceId, ErrorCode, Request};
use std::path::Path;

/// Marks `device` near or away on the daemon whose state lives in
/// `state_dir`. No daemon running, or one that does not know the device,
/// has no alerts of the phone's to hold: both are fine. Blocking.
pub fn set(state_dir: &Path, device: DeviceId, near: bool) -> super::Result<()> {
    let address = control::address(state_dir);
    match control::request(&address, &Request::PresenceSet { device, near }) {
        Ok(_)
        | Err(control::Error::Connect(_))
        | Err(control::Error::Daemon {
            code: ErrorCode::UnknownDevice,
            ..
        }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
