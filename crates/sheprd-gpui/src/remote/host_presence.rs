//! Presence on linked hosts (`docs/ios-app-plan.md`, "Presence, exactly",
//! Linked hosts). A phone near this computer is near its owner whichever
//! host an agent runs on, so each near mark this app gives its own daemon
//! also goes to every SSH or cloud machine the phone was paired with from
//! here, under the phone's id there. A host is told over SSH, as
//! `remote::links` enrolls a phone there, by running its
//! `sheprd-remote-control presence`, which marks the device on the daemon
//! running there. The host lets a near mark lapse after two minutes
//! without renewal, so a host this app stops reaching never holds alerts
//! for long.
//!
//! [`HostMarks`] is pure state: the station feeds it marks and runs what
//! it hands out, one run per host at a time, with [`send`] on the
//! background executor.

mod error;

use crate::teleport::Host;
pub(crate) use error::Error;
use sheprd_client::{ConnectTarget, ScriptLimits, shell_quote};
use sheprd_remote_control::control::{
    self, DeviceId, Response,
    hosts::{HostId, HostState},
};
use std::{
    collections::HashMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// How long after a failed run a host is tried again.
pub(crate) const RETRY: Duration = Duration::from_secs(30);
/// How long a mark is worth sending: a host lets a near mark lapse after
/// this, so an older one, near or away, changes nothing there.
pub(crate) const LAPSE: Duration = Duration::from_secs(120);
/// What a run may print: nothing, or an error line per mark.
const OUTPUT: u64 = 16 * 1024;
/// How long a host may say nothing.
const IDLE: Duration = Duration::from_secs(15);

/// A device's id on a host, as the host's daemon knows the phone.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct HostDevice {
    pub(crate) host: HostId,
    pub(crate) device: DeviceId,
}

/// One mark for a host's daemon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Mark {
    pub(crate) near: bool,
    /// When the station decided it, for [`LAPSE`].
    pub(crate) at: Instant,
}

/// A run to start: the marks for one host, by the phone's id there.
#[derive(Debug, PartialEq)]
pub(crate) struct Run {
    pub(crate) host: HostId,
    pub(crate) marks: HashMap<DeviceId, Mark>,
}

#[derive(Debug, Default)]
struct Owed {
    /// Marks to send, the newest per device.
    waiting: HashMap<DeviceId, Mark>,
    /// A run is out for this host.
    running: bool,
    /// Not before this, after a failed run.
    retry_at: Option<Instant>,
}

/// The marks owed to each linked host.
#[derive(Debug, Default)]
pub(crate) struct HostMarks {
    hosts: HashMap<HostId, Owed>,
}

impl HostMarks {
    /// Owes `to`'s host a mark for the phone's id there.
    pub(crate) fn mark(&mut self, to: HostDevice, near: bool, now: Instant) {
        self.hosts
            .entry(to.host)
            .or_default()
            .waiting
            .insert(to.device, Mark { near, at: now });
    }

    /// The runs to start now: each host with marks owed, none running,
    /// and not waiting to retry. Marks too old to matter are dropped.
    pub(crate) fn take(&mut self, now: Instant) -> Vec<Run> {
        let mut runs = Vec::new();
        for (host, state) in &mut self.hosts {
            state
                .waiting
                .retain(|_, mark| now.saturating_duration_since(mark.at) < LAPSE);
            let resting = state.retry_at.is_some_and(|at| now < at);
            if state.running || resting || state.waiting.is_empty() {
                continue;
            }
            state.running = true;
            state.retry_at = None;
            runs.push(Run {
                host: host.clone(),
                marks: std::mem::take(&mut state.waiting),
            });
        }
        self.hosts
            .retain(|_, state| state.running || !state.waiting.is_empty());
        runs
    }

    /// A run ended. A failed one's marks are owed again unless a newer
    /// mark for the device came meanwhile, after [`RETRY`]; a host this
    /// app no longer connects to is forgotten instead (`gone`).
    pub(crate) fn finished(&mut self, run: Run, outcome: Outcome, now: Instant) {
        let Some(state) = self.hosts.get_mut(&run.host) else {
            return;
        };
        state.running = false;
        match outcome {
            Outcome::Sent => {}
            Outcome::Failed => {
                for (device, mark) in run.marks {
                    state.waiting.entry(device).or_insert(mark);
                }
                state.retry_at = Some(now + RETRY);
            }
            Outcome::Gone => {
                self.hosts.remove(&run.host);
            }
        }
    }
}

/// How a run ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Sent,
    Failed,
    /// This app no longer connects to the host.
    Gone,
}

/// The phone's id on each host it was paired with from here, by its id
/// on this computer, from this daemon's answer to `devices`. Nothing when
/// the daemon did not answer: no host is marked then, and a near mark
/// there lapses.
pub(crate) fn paired(
    answer: Result<Response, control::Error>,
) -> HashMap<DeviceId, Vec<HostDevice>> {
    let Ok(Response::Devices { devices }) = answer else {
        return HashMap::new();
    };
    devices
        .into_iter()
        .map(|device| {
            let hosts = device
                .hosts
                .into_iter()
                .filter_map(|link| match link.state {
                    HostState::Paired(enrolled) => Some(HostDevice {
                        host: link.host,
                        device: enrolled.device,
                    }),
                    _ => None,
                })
                .collect();
            (device.id, hosts)
        })
        .collect()
}

/// The script that marks `marks` on a host's daemon. It finds the program
/// as `push-status` does, without installing it: a host without one runs
/// no daemon to hold alerts. It fails when any mark failed, so they are
/// sent again.
pub(crate) fn script(marks: &HashMap<DeviceId, Mark>) -> String {
    let mut script = String::from(crate::remote::program::find_program!());
    script.push_str("status=0\nif [ -x \"$program\" ]; then\n");
    for (device, mark) in marks {
        let flag = if mark.near { "--near" } else { "--away" };
        script.push_str(&format!(
            "  \"$program\" presence --device {} {flag} || status=1\n",
            shell_quote(&device.0)
        ));
    }
    script.push_str("fi\nexit $status\n");
    script
}

/// Marks `marks` on the host `target` reaches. Blocking.
pub(crate) fn send(target: &ConnectTarget, marks: &HashMap<DeviceId, Mark>) -> Result<(), Error> {
    let host = Host::new(target).map_err(|_| Error::Unsupported)?;
    host.stream(
        &script(marks),
        &[][..],
        std::io::sink(),
        ScriptLimits {
            output: OUTPUT,
            idle: IDLE,
        },
        &AtomicBool::new(false),
    )
    .map(drop)
    .map_err(Error::Run)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
