//! What each backend reports about its terminals: its agent's status, its
//! folder, and its name, for the chrome of split panes. Read on a short-lived
//! thread every few seconds while a split is shown, never on the UI thread.
//! The same read saves where each tmux shell moved, for a reboot to reopen.

use super::provider::Provider;
use sheprd_backend::{TerminalId, TerminalInfo};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{Receiver, TryRecvError, sync_channel},
    time::{Duration, Instant},
};

/// How often the list is read again.
const EVERY: Duration = Duration::from_secs(2);

type Inventory = Result<Vec<TerminalInfo>, super::Error>;

#[derive(Default)]
struct Listed {
    core: Option<Inventory>,
    tmux: Option<Inventory>,
}

#[derive(Default)]
pub(super) struct Infos {
    /// Terminals started since the last list was asked for, which a list
    /// already on its way may not name yet.
    fresh: HashSet<TerminalId>,
    known: HashMap<TerminalId, TerminalInfo>,
    last: Option<Instant>,
    /// When the list known was asked for.
    read: Option<Instant>,
    /// Lists read so far.
    lists: u64,
    core_read: Option<Instant>,
    tmux_available: bool,
    /// The list on its way, and when it was asked for.
    pending: Option<(Instant, Receiver<Listed>)>,
}

impl Infos {
    pub(super) fn get(&self, id: &TerminalId) -> Option<&TerminalInfo> {
        self.known.get(id)
    }

    /// The terminal `id` names: its address, or else its name, as a terminal
    /// found by name is shown under it.
    pub(super) fn named(&self, id: &TerminalId) -> Option<&TerminalInfo> {
        self.get(id).or_else(|| {
            // Only old core saves can hold a name in place of an address.
            if id.is_tmux() {
                return None;
            }
            self.known
                .values()
                .find(|info| !info.id.is_tmux() && info.name.as_deref() == Some(id.as_str()))
        })
    }

    /// How many of the core's terminals run an agent that is working.
    pub(super) fn working(&self) -> u32 {
        let working = self
            .known
            .values()
            .filter(|info| info.agent == sheprd_client::protocol::AgentStatus::Working)
            .count();
        u32::try_from(working).unwrap_or(u32::MAX)
    }

    /// Every terminal the core last reported.
    pub(super) fn all(&self) -> impl Iterator<Item = &TerminalInfo> {
        self.known.values()
    }

    /// Only records whose labels can be read or modified through herdr-core.
    pub(super) fn core_terminals(&self) -> impl Iterator<Item = &TerminalInfo> {
        self.all().filter(|info| !info.id.is_tmux())
    }

    /// When the list known was asked for; none before the first.
    pub(super) fn read_at(&self) -> Option<Instant> {
        self.read
    }

    pub(super) fn core_read_at(&self) -> Option<Instant> {
        self.core_read
    }

    pub(super) fn tmux_available(&self) -> bool {
        self.tmux_available
    }

    /// How many lists have been read; none means nothing is known yet.
    #[cfg(any(target_os = "macos", windows, test))]
    pub(super) fn lists(&self) -> u64 {
        self.lists
    }

    /// Takes `list` as a finished read would.
    #[cfg(test)]
    pub(super) fn list_for_test(&mut self, list: Vec<TerminalInfo>) {
        self.known = list
            .into_iter()
            .map(|info| (info.id.clone(), info))
            .collect();
        self.read = Some(Instant::now());
        self.core_read = self.read;
        self.lists += 1;
    }

    /// Records a terminal just started, until the next list replaces it,
    /// and hands it back.
    pub(super) fn insert(&mut self, info: TerminalInfo) -> TerminalInfo {
        self.fresh.insert(info.id.clone());
        self.known.insert(info.id.clone(), info.clone());
        info
    }

    pub(super) fn needs_poll(&self, now: Instant) -> bool {
        self.pending.is_some()
            || self
                .last
                .is_none_or(|last| now.duration_since(last) >= EVERY)
    }

    /// Takes a finished read and starts the next when one is due. True when
    /// what is known changed.
    pub(super) fn poll(
        &mut self,
        core: Option<Provider>,
        tmux: Option<Provider>,
        now: Instant,
    ) -> bool {
        let mut changed = false;
        if let Some((asked, pending)) = &self.pending {
            let asked = *asked;
            match pending.try_recv() {
                Ok(listed) => {
                    let mut succeeded = false;
                    for (tmux, result) in [(false, listed.core), (true, listed.tmux)] {
                        if tmux {
                            let available = result.as_ref().is_some_and(Result::is_ok);
                            changed |= available != self.tmux_available;
                            self.tmux_available = available;
                        } else if result.as_ref().is_some_and(Result::is_ok) {
                            self.core_read = Some(asked);
                        }
                        match result {
                            Some(Ok(list)) => {
                                succeeded = true;
                                let mut missing: HashSet<_> = self
                                    .known
                                    .keys()
                                    .filter(|id| id.is_tmux() == tmux)
                                    .cloned()
                                    .collect();
                                for info in list {
                                    missing.remove(&info.id);
                                    changed |= self.known.get(&info.id) != Some(&info);
                                    self.known.insert(info.id.clone(), info);
                                }
                                for id in missing.difference(&self.fresh) {
                                    self.known.remove(id);
                                    changed = true;
                                }
                                self.fresh.retain(|id| id.is_tmux() != tmux);
                            }
                            Some(Err(error)) => tracing::debug!(%error, "Could not list terminals"),
                            None => {}
                        }
                    }
                    if succeeded {
                        self.read = Some(asked);
                        self.lists += 1;
                    }
                    self.pending = None;
                }
                Err(TryRecvError::Disconnected) => self.pending = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let due = self
            .last
            .is_none_or(|last| now.duration_since(last) >= EVERY);
        if (core.is_some() || tmux.is_some()) && self.pending.is_none() && due {
            self.last = Some(now);
            let (send, receive) = sync_channel(1);
            // Where each tmux shell was last seen, so only one that moved is
            // saved again.
            let seen: HashMap<_, _> = self
                .known
                .values()
                .filter(|info| info.id.is_tmux())
                .map(|info| (info.id.clone(), info.cwd.clone()))
                .collect();
            let started = std::thread::Builder::new()
                .name("terminal-inventory".into())
                .spawn(move || {
                    let _ = send.send(Listed {
                        core: core.map(|b| b.terminals()),
                        tmux: tmux.map(|b| {
                            let listed = b.terminals();
                            if let Ok(list) = &listed {
                                b.keep_folders(list, &seen);
                            }
                            listed
                        }),
                    });
                });
            match started {
                Ok(_) => self.pending = Some((now, receive)),
                Err(error) => tracing::warn!(%error, "Could not start a terminal inventory worker"),
            }
        }
        changed
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
