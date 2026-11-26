//! Persisted guest OS, image identity, and versioned backend boot metadata.
//!
//! Each machine or image folder may hold `guest.json`. Folders created before
//! this file existed lack it and continue to mean an Apple macOS ARM64 guest.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::{Error, Result};
use serde::{Deserialize, Serialize};
pub(crate) use sheprd_control_protocol::protocol::tabs::{GuestArch, GuestOs, MachineCapabilities};
use std::{
    fs,
    io::{ErrorKind, Write as _},
    path::Path,
};

/// Current schema version of `guest.json`.
pub(crate) const METADATA_VERSION: u32 = 1;
/// File inside a machine or image folder describing its guest and boot format.
pub(crate) const FILE: &str = "guest.json";

const fn default_version() -> u32 {
    METADATA_VERSION
}

/// Identifies the image a machine was provisioned from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImageIdentity {
    pub(crate) id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sha256: Option<String>,
}

impl ImageIdentity {
    /// Human-readable image label (`id@version` or `id`).
    pub(crate) fn label(&self) -> String {
        match &self.version {
            Some(version) if !version.is_empty() => format!("{}@{version}", self.id), // i18n: data
            _ => self.id.clone(),
        }
    }
}

/// Versioned backend-specific boot metadata stored inside `guest.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BootMetadata {
    #[default]
    AppleMacos,
    AppleLinuxEfi {
        /// Whether to attach a virtio serial console (`hvc0`).
        #[serde(default)]
        serial_console: bool,
    },
}

/// Guest OS, architecture, image provenance, and backend boot metadata for a
/// machine or image folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GuestMetadata {
    #[serde(default = "default_version")]
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) os: GuestOs,
    #[serde(default)]
    pub(crate) arch: GuestArch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<ImageIdentity>,
    #[serde(default)]
    pub(crate) boot: BootMetadata,
}

impl Default for GuestMetadata {
    fn default() -> Self {
        Self::macos_default()
    }
}

impl GuestMetadata {
    /// Metadata assumed for legacy machine/image folders lacking `guest.json`.
    pub(crate) const fn macos_default() -> Self {
        Self {
            version: METADATA_VERSION,
            os: GuestOs::Macos,
            arch: GuestArch::Arm64,
            image: None,
            boot: BootMetadata::AppleMacos,
        }
    }

    /// Metadata for an ARM64 Linux EFI guest on Apple Virtualization.framework.
    pub(crate) fn linux_apple_efi(image: Option<ImageIdentity>, serial_console: bool) -> Self {
        Self {
            version: METADATA_VERSION,
            os: GuestOs::Linux,
            arch: GuestArch::Arm64,
            image,
            boot: BootMetadata::AppleLinuxEfi { serial_console },
        }
    }

    /// Lifecycle and display capabilities supported for this guest/backend pair.
    pub(crate) fn capabilities(&self) -> MachineCapabilities {
        match self.boot {
            BootMetadata::AppleMacos => MachineCapabilities::macos(),
            BootMetadata::AppleLinuxEfi { .. } => MachineCapabilities::linux_apple(),
        }
    }

    /// Image label for protocol listings, when recorded.
    pub(crate) fn image_label(&self) -> Option<String> {
        self.image.as_ref().map(ImageIdentity::label)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.version != METADATA_VERSION {
            return Err(Error::InvalidMetadata {
                path: path.to_owned(),
                reason: format!("unsupported metadata version {}", self.version), // i18n: data
            });
        }
        let consistent = matches!(
            (&self.os, &self.boot),
            (GuestOs::Macos, BootMetadata::AppleMacos)
                | (GuestOs::Linux, BootMetadata::AppleLinuxEfi { .. })
        );
        if !consistent {
            return Err(Error::InvalidMetadata {
                path: path.to_owned(),
                reason: format!("guest os `{}` does not match boot metadata", self.os), // i18n: data
            });
        }
        Ok(())
    }
}

/// Reads `guest.json` from `folder`. A missing file means a legacy macOS folder.
pub(crate) fn read_in(folder: &Path) -> Result<GuestMetadata> {
    let path = folder.join(FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == ErrorKind::NotFound => {
            return Ok(GuestMetadata::macos_default());
        }
        Err(source) => {
            return Err(Error::Read { path, source });
        }
    };
    let metadata: GuestMetadata =
        serde_json::from_slice(&bytes).map_err(|source| Error::InvalidMetadata {
            path: path.clone(),
            reason: source.to_string(),
        })?;
    metadata.validate(&path)?;
    Ok(metadata)
}

/// Atomically writes `guest.json` into `folder`.
pub(crate) fn write_in(folder: &Path, metadata: &GuestMetadata) -> Result<()> {
    let path = folder.join(FILE);
    metadata.validate(&path)?;
    let write_error = |source| Error::Write {
        path: path.clone(),
        source,
    };
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|source| write_error(source.into()))?;
    let mut partial = tempfile::NamedTempFile::new_in(folder).map_err(write_error)?;
    partial.write_all(&bytes).map_err(write_error)?;
    partial
        .persist(&path)
        .map(|_| ())
        .map_err(|error| write_error(error.error))
}

#[cfg(test)]
mod tests;
