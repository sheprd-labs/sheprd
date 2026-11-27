//! Phones agents may drive (`control::phone`): each live socket whose
//! phone said `agent_control`, and the drives waiting for its answer.
//! Sheprd's control thread asks and blocks for the answer; the socket's
//! session registers itself and hands answers back. A drive is answered
//! only by the socket it went to.

use crate::native::wire::ServerMessage;
use crate::{
    control::{
        ASK_WAIT, DeviceId,
        phone::{self, Answer, Ask, Command, Drive, MAX_PHONES, Outcome, Phone, Screen},
    },
    web::{State, now_ms},
};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError, mpsc as sync},
    time::Duration,
};
use tokio::sync::mpsc;

/// The live sockets of phones agents may drive, and the drives in flight.
#[derive(Default)]
pub(crate) struct Drivers {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The last socket or drive id handed out.
    last: u64,
    sockets: HashMap<u64, Socket>,
    /// Drives waiting for their phone, by drive id: the socket they went
    /// to, and where its answer goes.
    waiting: HashMap<u64, (u64, sync::SyncSender<Outcome>)>,
}

struct Socket {
    device: DeviceId,
    out: mpsc::Sender<ServerMessage>,
    active_ms: u64,
}

impl Inner {
    fn next(&mut self) -> u64 {
        self.last += 1;
        self.last
    }
}

impl Drivers {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new socket's id, which it registers under.
    pub(crate) fn socket(&self) -> u64 {
        self.inner().next()
    }

    /// The phone on `socket` lets agents drive it, or no longer does.
    pub(crate) fn allow(
        &self,
        socket: u64,
        allowed: Option<(DeviceId, mpsc::Sender<ServerMessage>)>,
        now_ms: u64,
    ) {
        let mut inner = self.inner();
        match allowed {
            Some((device, out)) => {
                inner.sockets.insert(
                    socket,
                    Socket {
                        device,
                        out,
                        active_ms: now_ms,
                    },
                );
            }
            None => inner.forget(socket),
        }
    }

    /// The phone on `socket` said something: it is the one in use.
    pub(crate) fn touch(&self, socket: u64, now_ms: u64) {
        if let Some(socket) = self.inner().sockets.get_mut(&socket) {
            socket.active_ms = now_ms;
        }
    }

    /// The phone on `socket` answered drive `id`. An answer to a drive
    /// that went to another socket, or that is no longer waited for, is
    /// dropped.
    pub(crate) fn answer(&self, socket: u64, id: u64, outcome: Outcome) {
        let mut inner = self.inner();
        if inner.waiting.get(&id).is_some_and(|(to, _)| *to == socket)
            && let Some((_, answer)) = inner.waiting.remove(&id)
        {
            let _ = answer.try_send(outcome);
        }
    }

    /// The socket closed.
    pub(crate) fn remove(&self, socket: u64) {
        self.inner().forget(socket);
    }

    /// Each device's most recently used socket, the latest first.
    fn phones(&self, name: impl Fn(&DeviceId) -> Option<String>) -> Vec<(u64, Phone)> {
        let inner = self.inner();
        let mut latest: HashMap<&DeviceId, (u64, &Socket)> = HashMap::new();
        for (id, socket) in &inner.sockets {
            let entry = latest.entry(&socket.device).or_insert((*id, socket));
            if (socket.active_ms, *id) > (entry.1.active_ms, entry.0) {
                *entry = (*id, socket);
            }
        }
        let mut phones: Vec<(u64, Phone)> = latest
            .into_values()
            .filter_map(|(id, socket)| {
                let phone = Phone {
                    name: name(&socket.device)?,
                    device: socket.device.clone(),
                    active_ms: socket.active_ms,
                };
                Some((id, phone))
            })
            .collect();
        phones.sort_by(|(a, one), (b, two)| (two.active_ms, b).cmp(&(one.active_ms, a)));
        phones.truncate(MAX_PHONES);
        phones
    }

    /// Sends `drive` to the phone on `socket` and waits up to `wait` for
    /// its answer. `None` when it did not answer, or could not be sent.
    fn drive(&self, socket: u64, drive: Drive, wait: Duration) -> Option<Outcome> {
        let (sent, answered) = sync::sync_channel(1);
        let id = {
            let mut inner = self.inner();
            let id = inner.next();
            let out = inner.sockets.get(&socket)?.out.clone();
            let message = ServerMessage::Drive { id, drive };
            out.try_send(message).ok()?;
            inner.waiting.insert(id, (socket, sent));
            id
        };
        let outcome = answered.recv_timeout(wait).ok();
        self.inner().waiting.remove(&id);
        outcome
    }
}

impl Inner {
    fn forget(&mut self, socket: u64) {
        self.sockets.remove(&socket);
        // Its drives end at once rather than wait out their time.
        self.waiting.retain(|_, (to, _)| *to != socket);
    }
}

/// Answers Sheprd's [`Ask`]. Blocking: waits up to [`ASK_WAIT`] for the
/// phone.
pub(crate) fn answer(state: &State, ask: Ask) -> Answer {
    let name = |device: &DeviceId| state.auth().devices().get(device).map(|d| d.name.clone());
    let phones = state.drivers.phones(name);
    let (device, command) = match ask {
        Ask::Phones => {
            return Answer::Phones {
                phones: phones.into_iter().map(|(_, phone)| phone).collect(),
            };
        }
        Ask::Drive { device, command } => (device, command),
    };
    let drive = match resolve(state, command) {
        Ok(drive) => drive,
        Err(answer) => return answer,
    };
    let (socket, phone) = match pick(phones, device.as_deref()) {
        Ok(found) => found,
        Err(answer) => return answer,
    };
    match state.drivers.drive(socket, drive, ASK_WAIT) {
        Some(outcome) => Answer::Driven { phone, outcome },
        None => Answer::NoAnswer { phone },
    }
}

/// The phone named `wanted`, by device id or name, else the one used last.
fn pick(phones: Vec<(u64, Phone)>, wanted: Option<&str>) -> Result<(u64, Phone), Answer> {
    let Some(wanted) = wanted else {
        return phones.into_iter().next().ok_or(Answer::NoPhone);
    };
    if let Some(found) = phones.iter().find(|(_, phone)| phone.device.0 == wanted) {
        return Ok(found.clone());
    }
    let mut named = phones.into_iter().filter(|(_, phone)| phone.name == wanted);
    match (named.next(), named.next()) {
        (Some(found), None) => Ok(found),
        (Some(_), Some(_)) => Err(Answer::Ambiguous),
        (None, _) => Err(Answer::NoPhone),
    }
}

/// What to tell the phone for `command`, checked against this computer's
/// layout, so a phone is never sent somewhere that is not there.
fn resolve(state: &State, command: Command) -> Result<Drive, Answer> {
    let layout = state.hub.layout();
    let to = match command {
        Command::State => return Ok(Drive::State),
        Command::Guide {
            text,
            highlight,
            from,
        } if phone::valid_text(&text) => {
            return Ok(Drive::Guide {
                text,
                highlight,
                from: from.and_then(|pane| pane_screen(state, &layout, pane)),
            });
        }
        Command::Guide { .. } => return Err(Answer::Invalid),
        Command::Highlight { target, from } => {
            return Ok(Drive::Highlight {
                target,
                from: from.and_then(|pane| pane_screen(state, &layout, pane)),
            });
        }
        Command::ClearGuide => return Ok(Drive::ClearGuide),
        Command::Show { text, from } if phone::valid_text(&text) => {
            return Ok(Drive::Show {
                text,
                from: from.and_then(|pane| pane_screen(state, &layout, pane)),
            });
        }
        Command::Show { .. } => return Err(Answer::Invalid),
        Command::OpenPane { pane } if !phone::valid_id(&pane) => return Err(Answer::Invalid),
        Command::OpenPane { pane } => match pane_screen(state, &layout, pane) {
            Some(screen) => screen,
            None => return Err(Answer::NotFound),
        },
        Command::Open { to } if !to.is_valid() => return Err(Answer::Invalid),
        Command::Open { to } => to,
    };
    let there = match &to {
        Screen::Chat { pane } => layout.chat(pane).is_some() || state.hub.observable(pane),
        Screen::Term { pane } => layout.terminal(pane).is_some(),
        Screen::Workspace { workspace } | Screen::Changes { workspace } => {
            layout.workspace(workspace).is_some()
        }
        Screen::Home
        | Screen::Workspaces
        | Screen::PullRequests
        | Screen::Settings
        | Screen::SettingsPage { .. }
        | Screen::NewWorkspace => true,
    };
    if !there {
        return Err(Answer::NotFound);
    }
    Ok(Drive::Open { to })
}

/// A pane as the phone shows it: a terminal agent's conversation is its
/// chat, and the phone falls back to the terminal otherwise. `None` for an
/// id out of bounds or a pane this computer does not list.
fn pane_screen(state: &State, layout: &crate::hub::Layout, pane: String) -> Option<Screen> {
    if !phone::valid_id(&pane) {
        None
    } else if layout.chat(&pane).is_some() || state.hub.observable(&pane) {
        Some(Screen::Chat { pane })
    } else if layout.terminal(&pane).is_some() {
        Some(Screen::Term { pane })
    } else {
        None
    }
}

/// A live socket's place among the drivers, for the life of its session:
/// dropping it forgets the socket and ends its drives.
pub(crate) struct Seat<'a> {
    drivers: &'a Drivers,
    pub(crate) id: u64,
}

impl<'a> Seat<'a> {
    pub(crate) fn new(drivers: &'a Drivers) -> Self {
        Self {
            id: drivers.socket(),
            drivers,
        }
    }

    pub(crate) fn allow(
        &self,
        allowed: bool,
        device: &DeviceId,
        out: &mpsc::Sender<ServerMessage>,
    ) {
        let allowed = allowed.then(|| (device.clone(), out.clone()));
        self.drivers.allow(self.id, allowed, now_ms());
    }
}

impl Drop for Seat<'_> {
    fn drop(&mut self) {
        self.drivers.remove(self.id);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
