//! Pinned ARM64 Linux desktop image catalog and atomic machine staging.
//!
//! Keeps Linux image provenance, checksum pins, default resource settings, and
//! `.install-*` staging/publication pure safe Rust so they are unit-testable
//! without touching Virtualization.framework.

use super::{Error, STAGING, forget_abandoned, qcow2, seed};
use crate::machine::{
    Display, Limits, Name, Result, Settings,
    configuration::{LINUX_DEFAULT_HEIGHT, LINUX_DEFAULT_WIDTH},
    metadata::{self, GuestMetadata, ImageIdentity},
    store::{self, DISK, EFI_VARS, IDENTITY, SEED_ISO},
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Default Linux desktop image identifier.
pub(crate) const DEFAULT_IMAGE_ID: &str = "ubuntu-wayland";

/// One pinned ARM64 Linux cloud image offered for provisioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogEntry {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) version: &'static str,
    pub(crate) url: &'static str,
    pub(crate) sha256: &'static str,
    /// Target sparse raw disk size after QCOW2 conversion.
    pub(crate) disk_bytes: u64,
    pub(crate) min_cpus: u16,
    pub(crate) min_memory_gib: u16,
    pub(crate) default_cpus: u16,
    pub(crate) default_memory_gib: u16,
}

/// Pinned Ubuntu 24.04.5 LTS ARM64 image validated in the Phase 1 spike.
pub(crate) const UBUNTU_WAYLAND: CatalogEntry = CatalogEntry {
    id: DEFAULT_IMAGE_ID,
    name: "Ubuntu 24.04.5 LTS (Wayland)",
    version: "24.04.5-20261017",
    url: "https://cloud-images.ubuntu.com/noble/20261017/noble-server-cloudimg-arm64.img",
    sha256: "50e24e1fb79e89116e939d90aa7738f7af48c442fa1bc2510131e0b39c50348e",
    disk_bytes: 16 << 30,
    min_cpus: 2,
    min_memory_gib: 2,
    default_cpus: 4,
    default_memory_gib: 4,
};

#[allow(dead_code)] // Listed by Settings UI and CLI in Phase 5.
pub(crate) const CATALOG: &[CatalogEntry] = &[UBUNTU_WAYLAND];

/// Looks up a pinned Linux image by `id`.
#[allow(dead_code)] // Called by `begin_linux` and Phase 5 CLI.
pub(crate) fn find(id: &str) -> std::result::Result<&'static CatalogEntry, Error> {
    CATALOG
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| Error::UnknownImage(id.to_owned()))
}

/// File name under `machine-downloads/` for `entry`.
pub(super) fn download_filename(entry: &CatalogEntry) -> String {
    format!("{}-{}.img", entry.id, entry.version) // i18n: data
}

/// Persisted image provenance recorded in `guest.json`.
pub(crate) fn image_identity(entry: &CatalogEntry) -> ImageIdentity {
    ImageIdentity {
        id: entry.id.to_owned(),
        version: Some(entry.version.to_owned()),
        sha256: Some(entry.sha256.to_owned()),
    }
}

/// Default settings for a new Linux desktop machine within `limits`.
///
/// Uses a fixed `1280x800` display matching the Phase 1 empirical findings
/// (dynamic resize is disabled at boot for Apple Virtio-GPU Linux guests).
pub(crate) fn first_settings(entry: &CatalogEntry, limits: &Limits) -> Settings {
    Settings {
        cpus: entry
            .default_cpus
            .clamp(*limits.cpus.start(), *limits.cpus.end())
            .max(entry.min_cpus),
        memory_gib: entry
            .default_memory_gib
            .clamp(*limits.memory_gib.start(), *limits.memory_gib.end())
            .max(entry.min_memory_gib),
        display: Display::Fixed {
            width: LINUX_DEFAULT_WIDTH,
            height: LINUX_DEFAULT_HEIGHT,
        },
    }
}

/// Inputs for staging a Linux machine into a `.install-*` folder.
pub(super) struct StageRequest<'a> {
    pub(super) root: &'a Path,
    pub(super) name: &'a Name,
    pub(super) entry: &'a CatalogEntry,
    pub(super) qcow2_path: &'a Path,
    pub(super) identity: &'a [u8],
    pub(super) limits: &'a Limits,
}

/// Prepares a Linux machine in a hidden `.install-*` staging folder inside
/// `request.root`: writes `identity.bin`, `guest.json`, `machine.json`,
/// `seed.iso`, creates `efi-vars.bin` via `create_efi`, and converts
/// `qcow2_path` into a sparse raw `disk.img`.
///
/// Because `TempDir` is returned to the caller, any failure or cancellation
/// deletes the staging folder automatically and leaves no incomplete machine in
/// `root`.
pub(super) fn prepare_in(
    request: &StageRequest<'_>,
    create_efi: impl FnOnce(&Path) -> Result<()>,
    cancel: &AtomicBool,
) -> Result<tempfile::TempDir> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled.into());
    }
    if request.root.join(request.name.as_str()).exists() {
        return Err(crate::machine::Error::Exists(request.name.clone()));
    }
    fs::create_dir_all(request.root).map_err(|source| store::write_error(request.root, source))?;
    forget_abandoned(request.root);
    let staging = tempfile::Builder::new()
        .prefix(STAGING)
        .tempdir_in(request.root)
        .map_err(|source| store::write_error(request.root, source))?;
    let folder = staging.path();

    let id_path = folder.join(IDENTITY);
    fs::write(&id_path, request.identity).map_err(|source| store::write_error(&id_path, source))?;

    let guest = GuestMetadata::linux_apple_efi(Some(image_identity(request.entry)), false);
    metadata::write_in(folder, &guest)?;
    store::write_settings(folder, &first_settings(request.entry, request.limits))?;

    let seed_path = folder.join(SEED_ISO);
    seed::write_provision_iso(&seed_path, request.name, request.identity)
        .map_err(|source| store::write_error(&seed_path, source))?;

    create_efi(&folder.join(EFI_VARS))?;

    qcow2::convert_to_raw(
        request.qcow2_path,
        &folder.join(DISK),
        request.entry.disk_bytes,
        cancel,
    )?;
    Ok(staging)
}

/// Atomically publishes staged machine folder `staging` to `root/name`.
pub(super) fn publish_in(staging: tempfile::TempDir, root: &Path, name: &Name) -> Result<PathBuf> {
    let target = root.join(name.as_str());
    let source = staging.path().to_owned();
    fs::rename(&source, &target).map_err(|error| match error.kind() {
        ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty => {
            crate::machine::Error::Exists(name.clone())
        }
        _ => store::write_error(&target, error),
    })?;
    let _ = staging.keep();
    Ok(target)
}

#[cfg(test)]
mod tests;
