//! Where a prompt was typed, so the chat program can tell the agent what
//! the person in front of it can see and open.

use serde::{Deserialize, Serialize};

/// The device and the connection a prompt came from. The client that
/// relays a prompt sets it, never the person typing: the remote control
/// daemon replaces whatever a phone claims.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    pub device: Device,
    pub via: Via,
    /// The authenticated phone's id on the computer receiving this prompt.
    /// Older clients omit it. Never a bearer token or a device name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Someone other than the person wrote it, relayed by a tool: set only
    /// by the chat program, for a guest prompt the person allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<crate::Guest>,
}

impl Origin {
    pub const fn new(device: Device, via: Via) -> Self {
        Self {
            device,
            via,
            phone: None,
            guest: None,
        }
    }

    /// A prompt `guest` wrote, relayed into the chat by a tool.
    pub fn guest(guest: crate::Guest) -> Self {
        Self {
            guest: Some(guest),
            ..Self::new(Device::Other, Via::Other)
        }
    }

    /// Whether the person can open a path on the agent's machine where they
    /// typed: only on that machine, in the desktop app or its terminal.
    /// Never for a guest.
    pub fn opens_paths(&self) -> bool {
        self.guest.is_none()
            && self.via == Via::Local
            && matches!(self.device, Device::Desktop | Device::Terminal)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    /// Sheprd's desktop app.
    Desktop,
    /// Sheprd's app on an iPhone or iPad.
    PhoneApp,
    /// Sheprd's web app in a phone's browser.
    PhoneWeb,
    /// The chat view in the terminal pane the chat program runs in.
    Terminal,
    // Serde's forward-compatible fallback must be last.
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// The local network.
    Lan,
    /// The computer the agent runs on.
    Local,
    /// Sheprd's end-to-end encrypted relay.
    Relay,
    /// Another computer, through Sheprd's connection to it: SSH, a cloud
    /// machine, or WSL.
    Remote,
    Tailscale,
    // Serde's forward-compatible fallback must be last.
    #[serde(other)]
    Other,
}
