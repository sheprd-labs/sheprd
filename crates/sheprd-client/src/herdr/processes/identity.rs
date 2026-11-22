//! What tells one process apart from a later one given the same PID: its
//! birth, read on the host it runs on, with its parent and group.

/// A process's parent, process group, and birth. `started` is clock ticks
/// since boot on Linux and microseconds since the epoch on macOS; it is
/// only ever compared with another reading on the same host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub parent: u32,
    pub group: u32,
    pub started: u64,
}

/// Bytes of `/proc/PID/stat` read; the start time is its 22nd field.
#[cfg(target_os = "linux")]
const MAX_STAT: u64 = 8192;

/// `pid` on this host, as `/proc/PID/stat` gives it; `None` when it is gone
/// or cannot be read.
#[cfg(target_os = "linux")]
pub fn local_identity(pid: u32) -> Option<ProcessIdentity> {
    use std::io::Read;
    let mut stat = String::new();
    std::fs::File::open(format!("/proc/{pid}/stat"))
        .ok()?
        .take(MAX_STAT)
        .read_to_string(&mut stat)
        .ok()?;
    parse_stat(&stat)
}

/// The parent, group and start ticks in a `/proc/PID/stat` line. The
/// command name before them may hold spaces or `)`, so the fields are read
/// after its last `)`.
#[cfg(any(target_os = "linux", test))]
fn parse_stat(stat: &str) -> Option<ProcessIdentity> {
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some(ProcessIdentity {
        parent: fields.get(1)?.parse().ok()?,
        group: fields.get(2)?.parse().ok()?,
        started: fields.get(19)?.parse().ok()?,
    })
}

/// `pid` on this Mac, from the kernel's record of it; `None` when it is
/// gone or cannot be read.
#[cfg(target_os = "macos")]
pub fn local_identity(pid: u32) -> Option<ProcessIdentity> {
    use libproc::libproc::{bsd_info::BSDInfo, proc_pid::pidinfo};
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let info = pidinfo::<BSDInfo>(pid, 0).ok()?;
    Some(ProcessIdentity {
        parent: info.pbi_ppid,
        group: info.pbi_pgid,
        started: info
            .pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.pbi_start_tvusec),
    })
}

/// No other host's process metadata is read here.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn local_identity(_pid: u32) -> Option<ProcessIdentity> {
    None
}

#[cfg(test)]
mod tests;
