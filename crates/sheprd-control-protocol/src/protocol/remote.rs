//! Remote control: the daemon's status and devices, how its iPhone pushes
//! leave it (the push relay), the pairing dialog, the revoke confirmation,
//! restarting the daemon, and keeping a private relay's listen token. The daemon's own types are reused as they are;
//! no pairing token ever crosses this socket, and a listen token crosses
//! it only from the caller, once, to be kept: no answer carries it back.
use serde::{Deserialize, Serialize};
use sheprd_remote_control::control::{Device, Status, push::PushStatus, relays::ListenToken};

/// Asking the daemon, and for `revoke` the device list first, takes up to
/// its own answer timeout a request.
const DAEMON_WAIT_SECONDS: u64 = 12;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteOp {
    Status,
    /// Opens the pairing dialog in the window.
    Pair {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        address: Option<String>,
    },
    Devices,
    /// How iPhone pushes leave the daemon (off, own APNs key, or push
    /// relay) and how the last of each kind went. Read-only: the push
    /// relay is turned on in Settings.
    PushStatus,
    /// Opens the confirmation for revoking a device, by ID or name.
    Revoke {
        device: String,
    },
    /// Restarts the daemon from the binary beside the app.
    Restart,
    /// Keeps this computer's listen token for the relay at `relay`, or
    /// without one forgets it. `Debug` never shows the token.
    RelayToken {
        relay: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<ListenToken>,
    },
}

impl RemoteOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Status
            | Self::Devices
            | Self::PushStatus
            | Self::Revoke { .. }
            | Self::RelayToken { .. } => DAEMON_WAIT_SECONDS,
            Self::Pair { .. } | Self::Restart => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Status {
        /// Whether remote control is on in Settings.
        enabled: bool,
        /// What the daemon says, when it answers.
        #[serde(default)]
        status: Option<Box<Status>>,
        /// Why it did not, when it is on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Devices {
        devices: Vec<Device>,
    },
    PushStatus {
        /// Whether remote control is on in Settings.
        enabled: bool,
        /// Whether `[remote] push_relay` allows the push relay. An own
        /// APNs key wins over it, so `status` may still say `own_key`.
        push_relay: bool,
        /// What the daemon says, when it answers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<PushStatus>,
        /// Why it did not, when remote control is on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// A listen token was kept or forgotten for `relay`, as the app
    /// normalized its address; `token` is whether one is now kept.
    RelayToken {
        relay: String,
        token: bool,
    },
}

#[cfg(test)]
mod tests;
