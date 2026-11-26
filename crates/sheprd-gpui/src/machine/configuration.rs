//! What a machine boots from: its files on disk, guest metadata, and
//! deterministic device derivation.
//!
//! Framework-specific configuration building lives in [`super::backend::apple`];
//! this module stays pure safe Rust so bundle reading and validation can be
//! tested without touching Virtualization.framework.

use super::{
    Error, Name, Result, Settings, backend,
    guide::Opening,
    metadata::{self, BootMetadata, GuestMetadata, MachineCapabilities},
    store::{
        self, AUXILIARY, DISK, EFI_VARS, FROM_IMAGE, GUIDE, HARDWARE, IDENTITY, SEED_ISO,
        SERIAL_LOG,
    },
};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A macOS display that follows its tab starts at this size.
pub(super) const FIT_WIDTH: u16 = 2560;
pub(super) const FIT_HEIGHT: u16 = 1600;
/// Fixed starting size for Linux guests when display is `fit` until dynamic
/// resize is enabled.
pub(super) const LINUX_DEFAULT_WIDTH: u16 = 1280;
pub(super) const LINUX_DEFAULT_HEIGHT: u16 = 800;

/// The machine's network address, the same at each launch: left to itself the
/// framework picks a new one, and a saved state then refuses to restore. It
/// comes from the machine's identity, so it needs no file of its own. Marked
/// locally administered and unicast, as an address no vendor owns must be.
pub(super) fn mac_address(identity: &[u8]) -> String {
    let digest = Sha256::digest(identity);
    let first = (digest[0] | 0b10) & !0b1;
    format!(
        "{first:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        digest[1], digest[2], digest[3], digest[4], digest[5]
    ) // i18n: data
}

/// The machine's copy of the installer disk: a resumed guest may have it
/// mounted, so its blocks must not change under it while it is saved.
const INSTALLER: &str = "agent-installer.img";

/// The installer disk of Sheprd Agent in the app's bundle, beside its
/// executable. A build run outside a bundle has none.
fn bundled_installer() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let disk = executable
        .parent()?
        .parent()?
        .join("Resources")
        .join("MachineAgent.img");
    disk.is_file().then_some(disk)
}

/// The installer disk machine `folder` boots with. A fresh boot takes the
/// app's current one, `bundled`; a resume keeps the copy it was saved with,
/// as the guest saw it. A machine boots without one rather than not at all.
fn installer(folder: &Path, bundled: Option<&Path>, resuming: bool) -> Option<PathBuf> {
    let copy = folder.join(INSTALLER);
    if !resuming {
        let bundled = bundled?;
        // `fs::copy` clones on APFS; the old copy goes first, as a clone
        // cannot replace a file.
        let copied = fs::remove_file(&copy)
            .or_else(|error| match error.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(error),
            })
            .and_then(|()| fs::copy(bundled, &copy));
        if let Err(error) = copied {
            tracing::warn!(path = %copy.display(), %error, "Cannot copy the agent installer");
            return None;
        }
    }
    copy.is_file().then_some(copy)
}

/// A macOS machine identifier no other machine has, as its file holds it.
pub(super) fn new_identity() -> Vec<u8> {
    new_identity_for(&BootMetadata::AppleMacos)
}

/// A machine identifier no other machine has for guest boot type `boot`.
pub(super) fn new_identity_for(boot: &BootMetadata) -> Vec<u8> {
    backend::apple::new_identity(boot)
}

/// Guest-specific boot artifacts loaded from a machine's folder.
pub(super) enum BootFiles {
    Macos {
        hardware: Vec<u8>,
        identity: Vec<u8>,
        auxiliary: PathBuf,
        /// The installer disk of Sheprd Agent, attached read-only.
        installer: Option<PathBuf>,
    },
    LinuxEfi {
        identity: Vec<u8>,
        efi_vars: PathBuf,
        /// Optional NoCloud `cidata` ISO attached read-only for provisioning
        /// and clone identity regeneration.
        seed_iso: Option<PathBuf>,
        /// Optional virtio serial console output log (`hvc0`).
        serial_log: Option<PathBuf>,
    },
}

/// A machine's files: the small ones read, the large ones found.
pub(super) struct Bundle {
    pub(super) guest: GuestMetadata,
    pub(super) boot: BootFiles,
    pub(super) disk: PathBuf,
    pub(super) settings: Settings,
    /// The saved state to resume from, if it is suspended and save/restore is
    /// supported for this guest.
    pub(super) saved: Option<PathBuf>,
    /// How its setup guide opens, while it is still open as when it was
    /// just made.
    pub(super) guide: Option<Opening>,
}

impl Bundle {
    /// Reads machine `name`'s folder. It reaches the disk, so it runs off the
    /// main thread.
    pub(super) fn read(name: &Name) -> Result<Self> {
        store::require_installed_name(name)?;
        Self::read_in(&store::folder(name)?, true)
    }

    /// Reads the machine in `folder`, with the agent's installer disk or,
    /// while macOS is being installed, without it.
    pub(super) fn read_in(folder: &Path, with_installer: bool) -> Result<Self> {
        let folder = folder.to_owned();
        let disk = folder.join(DISK);
        if !disk.is_file() {
            return Err(Error::NotInstalled(folder));
        }
        let guest = metadata::read_in(&folder)?;
        let capabilities = guest.capabilities();
        let saved = capabilities
            .save_restore
            .then(|| folder.join(store::STATE))
            .filter(|state| state.is_file());
        let (boot, guide) = match &guest.boot {
            BootMetadata::AppleMacos => {
                let auxiliary = folder.join(AUXILIARY);
                if !auxiliary.is_file() {
                    return Err(Error::NotInstalled(folder));
                }
                let installer = with_installer
                    .then(|| installer(&folder, bundled_installer().as_deref(), saved.is_some()))
                    .flatten();
                (
                    BootFiles::Macos {
                        hardware: read(&folder.join(HARDWARE))?,
                        identity: read(&folder.join(IDENTITY))?,
                        auxiliary,
                        installer,
                    },
                    guide(&folder.join(GUIDE))?,
                )
            }
            BootMetadata::AppleLinuxEfi { serial_console } => {
                let efi_vars = folder.join(EFI_VARS);
                if !efi_vars.is_file() {
                    return Err(Error::NotInstalled(folder));
                }
                let seed_iso = folder.join(SEED_ISO);
                let seed_iso = seed_iso.is_file().then_some(seed_iso);
                let serial_log = serial_console.then(|| folder.join(SERIAL_LOG));
                (
                    BootFiles::LinuxEfi {
                        identity: read(&folder.join(IDENTITY))?,
                        efi_vars,
                        seed_iso,
                        serial_log,
                    },
                    None,
                )
            }
        };
        Ok(Self {
            guest,
            boot,
            disk,
            settings: store::boot_settings(&folder, capabilities.save_restore)?,
            guide,
            saved,
        })
    }

    pub(super) fn capabilities(&self) -> MachineCapabilities {
        self.guest.capabilities()
    }
}

/// How the guide whose marker is at `path` opens, if it is there.
fn guide(path: &Path) -> Result<Option<Opening>> {
    match fs::read(path) {
        Ok(marker) if marker == FROM_IMAGE => Ok(Some(Opening::Quiet)),
        Ok(_) => Ok(Some(Opening::Shown)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(store::read_error(path, source)),
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => {
            Error::NotInstalled(path.parent().unwrap_or(path).to_owned())
        }
        _ => store::read_error(path, source),
    })
}

#[cfg(test)]
mod tests;
