//! The observed agent's process, as far as identifying its transcript needs:
//! its birth time, working directory, open files, and environment. Linux
//! reads `/proc`, which tests fake on every host; macOS asks the kernel.

use super::{Error, Result};
use std::path::PathBuf;

/// Bytes of the environment read; only one config variable is kept.
#[cfg(any(target_os = "linux", test))]
const MAX_ENVIRONMENT: u64 = 128 * 1024;

pub(super) enum Process {
    /// A `/proc/PID` directory: Linux, and tests on every host.
    #[cfg(any(target_os = "linux", test))]
    Proc(PathBuf),
    /// A process on this Mac, by PID.
    #[cfg(target_os = "macos")]
    Darwin(u32),
    /// Elsewhere no process can be read, so none is ever made; the variant
    /// keeps every `match` here whole on every platform. Never constructing
    /// it is its purpose.
    #[cfg(not(any(target_os = "linux", target_os = "macos", test)))]
    #[allow(dead_code)]
    Unsupported(std::convert::Infallible),
}

impl Process {
    /// The process `pid` on this host.
    #[cfg(target_os = "linux")]
    pub(super) fn local(pid: u32) -> Result<Self> {
        Ok(Self::Proc(
            std::path::Path::new("/proc").join(pid.to_string()),
        ))
    }

    /// The process `pid` on this host.
    #[cfg(target_os = "macos")]
    pub(super) fn local(pid: u32) -> Result<Self> {
        Ok(Self::Darwin(pid))
    }

    /// The process `pid` on this host.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(super) fn local(_pid: u32) -> Result<Self> {
        Err(Error::Unsupported)
    }

    /// When the process started, which a reused PID does not share: clock
    /// ticks since boot on Linux, microseconds since the epoch on macOS. A
    /// process that no longer exists is an `io::ErrorKind::NotFound`.
    pub(super) fn started(&self) -> Result<u64> {
        match self {
            #[cfg(any(target_os = "linux", test))]
            Self::Proc(path) => procfs::started(path),
            #[cfg(target_os = "macos")]
            Self::Darwin(pid) => darwin::started(*pid),
            #[cfg(not(any(target_os = "linux", target_os = "macos", test)))]
            Self::Unsupported(never) => match *never {},
        }
    }

    pub(super) fn cwd(&self) -> Option<PathBuf> {
        match self {
            #[cfg(any(target_os = "linux", test))]
            Self::Proc(path) => std::fs::read_link(path.join("cwd")).ok(),
            #[cfg(target_os = "macos")]
            Self::Darwin(pid) => darwin::cwd(*pid),
            #[cfg(not(any(target_os = "linux", target_os = "macos", test)))]
            Self::Unsupported(never) => match *never {},
        }
    }

    /// The files the process holds open, bounded. macOS reports none: its
    /// transcripts are found by exact name instead, and a descriptor is only
    /// ever a candidate that the file's own header must confirm.
    pub(super) fn open_files(&self) -> Vec<PathBuf> {
        match self {
            #[cfg(any(target_os = "linux", test))]
            Self::Proc(path) => procfs::open_files(path),
            #[cfg(target_os = "macos")]
            Self::Darwin(_) => Vec::new(),
            #[cfg(not(any(target_os = "linux", target_os = "macos", test)))]
            Self::Unsupported(never) => match *never {},
        }
    }

    /// The environment as `NAME=value` entries separated by NUL bytes, at
    /// most 128 KiB; empty when it cannot be read. macOS
    /// shows no other process's environment, even this user's, so there an
    /// agent's config directory is always its default.
    pub(super) fn environment(&self) -> Result<Vec<u8>> {
        match self {
            #[cfg(any(target_os = "linux", test))]
            Self::Proc(path) => {
                let bytes = procfs::environment(path)?;
                if bytes.len() as u64 > MAX_ENVIRONMENT {
                    return Err(Error::EnvironmentTooLarge);
                }
                Ok(bytes)
            }
            #[cfg(target_os = "macos")]
            Self::Darwin(_) => Ok(Vec::new()),
            #[cfg(not(any(target_os = "linux", target_os = "macos", test)))]
            Self::Unsupported(never) => match *never {},
        }
    }
}

#[cfg(any(target_os = "linux", test))]
mod procfs {
    use super::{Error, MAX_ENVIRONMENT, Result};
    use std::{
        fs::{self, File},
        io::{self, Read},
        path::{Path, PathBuf},
    };

    /// Open descriptors inspected for transcript candidates.
    const MAX_FDS: usize = 2048;
    /// Bytes of `/proc/PID/stat` read; the start time is its 22nd field.
    const MAX_STAT: u64 = 8192;

    pub(super) fn started(process: &Path) -> Result<u64> {
        let mut stat = String::new();
        File::open(process.join("stat"))?
            .take(MAX_STAT)
            .read_to_string(&mut stat)?;
        // comm may contain spaces and parentheses; fields resume after its last ')'.
        let rest = stat.rsplit_once(')').ok_or(Error::InvalidStat)?.1;
        rest.split_whitespace()
            .nth(19)
            .ok_or(Error::MissingStartTime)?
            .parse()
            .map_err(Error::InvalidStartTime)
    }

    pub(super) fn open_files(process: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(process.join("fd")) else {
            return Vec::new();
        };
        entries
            .take(MAX_FDS)
            .flatten()
            .filter_map(|entry| fs::read_link(entry.path()).ok())
            .collect()
    }

    /// One byte past the limit, so the caller can tell it was exceeded.
    pub(super) fn environment(process: &Path) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        match File::open(process.join("environ")) {
            Ok(file) => {
                file.take(MAX_ENVIRONMENT + 1).read_to_end(&mut bytes)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(bytes)
    }
}

#[cfg(target_os = "macos")]
mod darwin {
    use super::{Error, Result};
    use std::{io, path::PathBuf};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    /// Microseconds since the epoch, read as everything else here reads a
    /// Mac process's birth ([`sheprd_client::herdr::processes::local_identity`]).
    pub(super) fn started(pid: u32) -> Result<u64> {
        let raw = i32::try_from(pid)
            .ok()
            .filter(|pid| *pid > 0)
            .ok_or(Error::InvalidPid)?;
        match sheprd_client::herdr::processes::local_identity(pid) {
            Some(identity) => Ok(identity.started),
            None if gone(raw) => Err(io::Error::from(io::ErrorKind::NotFound).into()),
            None => Err(Error::ProcessInfo),
        }
    }

    /// Only whether the PID names no process; a refused probe is not proof.
    fn gone(pid: i32) -> bool {
        rustix::process::Pid::from_raw(pid).is_some_and(|pid| {
            rustix::process::test_kill_process(pid) == Err(rustix::io::Errno::SRCH)
        })
    }

    /// The working directory, which the kernel shows only for this user's
    /// own processes.
    pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
        let pid = Pid::from_u32(pid);
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            false,
            ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always),
        );
        system.process(pid)?.cwd().map(PathBuf::from)
    }
}
