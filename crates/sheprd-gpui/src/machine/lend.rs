//! Whether this Mac lends its virtual Macs to the user's other computers,
//! and how many VMs it really runs. macOS allows two VMs at once whoever
//! runs them, so the VMs of other programs, such as Tart's CI runners, fill
//! the same slots; a Mac whose slots others use never lends, as a borrower
//! could be promised a slot a CI job takes a minute later.
//!
//! Everything here reads processes or the disk, so it runs off the main
//! thread.

use crate::control::protocol::tabs::{Lending, MachineCapacity};
use std::{ffi::OsStr, path::PathBuf};

/// The process Virtualization.framework runs for each VM, whichever app
/// started it.
const VM_PROCESS: &str = "com.apple.Virtualization.VirtualMachine";

/// What the computer runs and keeps, read once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Survey {
    /// Every VM running, Sheprd's included.
    pub(crate) vms: u32,
    /// Whether Tart keeps VMs, running or not.
    pub(crate) tart: bool,
}

impl Survey {
    /// Reads the processes and Tart's folder.
    pub(crate) fn read() -> Self {
        Self {
            vms: running_vms(),
            tart: tart_keeps_vms(),
        }
    }

    /// The slots in use, given the `sheprd` slots Sheprd's own machines and
    /// installer take. A VM Sheprd starts shows as a process a moment after
    /// it counts it, so the larger of the two counts wins.
    pub(crate) fn capacity(self, sheprd: u32) -> MachineCapacity {
        MachineCapacity::with_others(sheprd, self.vms.saturating_sub(sheprd))
    }

    /// Whether this Mac lends, with lending switched `on` in its settings.
    pub(crate) fn lending(self, on: bool, sheprd: u32) -> Lending {
        let others = self.capacity(sheprd).others;
        if self.tart {
            Lending::Tart
        } else if others > 0 {
            Lending::OtherVms { count: others }
        } else if !on {
            Lending::Off
        } else {
            Lending::Lends
        }
    }
}

fn running_vms() -> u32 {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    let count = system
        .processes()
        .values()
        .filter(|process| {
            process
                .exe()
                .and_then(|exe| exe.file_name())
                .is_some_and(|name| name == OsStr::new(VM_PROCESS))
        })
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Where Tart keeps its VMs: `TART_HOME`, else `~/.tart`.
fn tart_home() -> Option<PathBuf> {
    std::env::var_os("TART_HOME")
        .map(PathBuf::from)
        .or_else(|| sheprd_client::local_path::home_dir().map(|home| home.join(".tart")))
}

fn tart_keeps_vms() -> bool {
    tart_home().is_some_and(|home| keeps_vms(&home.join("vms")))
}

/// Whether `folder` holds a VM: any entry that is not hidden.
fn keeps_vms(folder: &std::path::Path) -> bool {
    std::fs::read_dir(folder).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
    })
}

#[cfg(test)]
mod tests;
