//! Guest operating system, architecture, and capability types shared across
//! local machine management, the control protocol, and remote forwarding.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The operating system running inside a virtual machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestOs {
    // keep-sorted start
    Linux,
    #[default]
    Macos,
    // keep-sorted end
}

impl GuestOs {
    /// The OS name as JSON and CLI output spell it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
        }
    }

    /// Parses a guest OS flag or wire name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "linux" => Some(Self::Linux),
            "macos" | "mac" => Some(Self::Macos),
            _ => None,
        }
    }
}

impl fmt::Display for GuestOs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// The CPU architecture of a virtual machine guest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestArch {
    // keep-sorted start
    #[default]
    Arm64,
    X86_64,
    // keep-sorted end
}

impl GuestArch {
    /// The architecture name as JSON and CLI output spell it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X86_64 => "x86_64",
        }
    }

    /// Parses an architecture name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "arm64" | "aarch64" => Some(Self::Arm64),
            "x86_64" | "amd64" => Some(Self::X86_64),
            _ => None,
        }
    }
}

impl fmt::Display for GuestArch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Lifecycle and display operations supported by a VM's host backend and guest
/// configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineCapabilities {
    /// Whether the VM can suspend to disk and restore later.
    pub save_restore: bool,
    /// Whether the VM accepts a graceful shutdown signal (e.g. ACPI power button).
    pub graceful_shutdown: bool,
    /// Whether the guest display can dynamically follow tab size (`fit`).
    pub dynamic_display: bool,
}

impl MachineCapabilities {
    /// Capabilities of a macOS guest on Apple Virtualization.framework.
    pub const fn macos() -> Self {
        Self {
            save_restore: true,
            graceful_shutdown: true,
            dynamic_display: true,
        }
    }

    /// Capabilities of an ARM64 Linux EFI guest on Apple Virtualization.framework.
    ///
    /// Save/restore is explicitly disabled (restore fails with VZErrorDomain
    /// Code=12 on Linux EFI); quit gracefully shuts down via ACPI and reopen
    /// cold-boots. Display begins in fixed mode.
    pub const fn linux_apple() -> Self {
        Self {
            save_restore: false,
            graceful_shutdown: true,
            dynamic_display: false,
        }
    }
}

impl Default for MachineCapabilities {
    /// Defaults to macOS capabilities so payloads from older hosts remain accurate.
    fn default() -> Self {
        Self::macos()
    }
}

/// OS-neutral capabilities reported by a guest agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub exec: bool,
    pub screenshot: bool,
    pub pointer: bool,
    pub keyboard: bool,
    pub text: bool,
    pub desktop_ready: bool,
}

impl AgentCapabilities {
    /// Derives common capabilities from legacy macOS permission booleans when
    /// talking to an older macOS guest agent.
    pub const fn from_macos_permissions(accessibility: bool, screen_recording: bool) -> Self {
        Self {
            exec: true,
            screenshot: screen_recording,
            pointer: accessibility,
            keyboard: accessibility,
            text: accessibility,
            desktop_ready: true,
        }
    }
}
