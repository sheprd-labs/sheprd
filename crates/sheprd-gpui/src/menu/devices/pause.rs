//! Pausing a saved device: it stays listed in the sidebar, greyed, but
//! nothing connects to it, so it neither shows its workspaces nor notifies,
//! until it is resumed. The device's own Herdr and terminals keep running.
//! SSH devices keep this as their catalog profile's `enabled` flag, which
//! `herdr machine enable|disable` writes; cloud devices in the app's own list.
//!
//! Requests for one device are queued: one write runs at a time, and the
//! latest request is written after it, so a quick pause then resume never
//! lands out of order. Until the catalog shows the outcome, the device reads
//! as the latest request asked.
use super::setup;
use crate::{SheprdWindow, i18n::fl, window::Flash};
use gpui::Context;
use sheprd_client::ConnectTarget;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// How long a finished request outlives the catalog not showing it, as when
/// someone else edits the catalog meanwhile.
const SETTLE: Duration = Duration::from_secs(10);

/// A request's number, for following its outcome.
pub(crate) type Ticket = u64;

/// Every device's queued pause requests in this window.
#[derive(Default)]
pub(crate) struct Pauses {
    next: Ticket,
    devices: HashMap<String, Pending>,
}

struct Pending {
    /// Paused (`true`) or resumed, as the latest request asked.
    wanted: bool,
    /// The latest request.
    ticket: Ticket,
    /// The request being written, if any.
    writing: Option<Ticket>,
    /// The latest request whose state was saved.
    saved: Option<Ticket>,
    /// The latest failed request and why.
    failed: Option<(Ticket, String)>,
    /// When the last write ended.
    ended: Option<Instant>,
}

impl Pending {
    fn idle(&self) -> bool {
        self.writing.is_none()
    }

    /// Whether the latest request ended in failure, so the catalog's own
    /// flag says again what the device is.
    fn latest_failed(&self) -> bool {
        self.failed
            .as_ref()
            .is_some_and(|(ticket, _)| *ticket == self.ticket)
    }
}

/// How a request ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Saved,
    Failed(String),
}

/// What a pause changes, read off the endpoint before it runs in the
/// background.
enum Pausable {
    Ssh(String),
    #[cfg(feature = "cloud")]
    Cloud {
        provider: crate::cloud::CloudProvider,
        id: String,
    },
}

impl SheprdWindow {
    /// Whether the saved device `id` is paused, or asked to be by a request
    /// the catalog does not show yet. Local and unknown devices never are.
    pub(crate) fn device_paused(&self, id: &str) -> bool {
        let Some(endpoint) = self.endpoints[1..]
            .iter()
            .find(|endpoint| endpoint.id == id)
        else {
            return false;
        };
        match self.menu.pauses.devices.get(id) {
            Some(pending)
                if !pending.idle()
                    || (!pending.latest_failed()
                        && pending.ended.is_some_and(|ended| ended.elapsed() < SETTLE)) =>
            {
                pending.wanted
            }
            _ => !endpoint.enabled,
        }
    }

    /// Whether the device `id` is a saved one a pause applies to: a cloud
    /// device, or an SSH device while its catalog is the one `herdr machine`
    /// writes (not a development one or an explicit socket's).
    pub(crate) fn device_pausable(&self, id: &str) -> bool {
        self.pausable(id).is_some()
    }

    fn pausable(&self, id: &str) -> Option<Pausable> {
        let endpoint = self.endpoints[1..]
            .iter()
            .find(|endpoint| endpoint.id == id)?;
        match endpoint.saved_target() {
            ConnectTarget::Ssh { .. } if self.device_setup_unavailable().is_none() => {
                crate::endpoint::saved_profile_id(id).map(|profile| Pausable::Ssh(profile.into()))
            }
            #[cfg(feature = "cloud")]
            ConnectTarget::Cloud { provider, id, .. } => Some(Pausable::Cloud {
                provider: *provider,
                id: id.clone(),
            }),
            _ => None,
        }
    }

    /// Asks to pause (`paused`) or resume the saved device `id`. The write
    /// runs in the background after any already running for the device;
    /// every window's catalog watcher then stops or restarts its connection.
    /// `None`, changing nothing, for a device that cannot be paused.
    pub(crate) fn set_device_paused(
        &mut self,
        id: &str,
        paused: bool,
        cx: &mut Context<Self>,
    ) -> Option<Ticket> {
        self.pausable(id)?;
        let pauses = &mut self.menu.pauses;
        pauses.next += 1;
        let ticket = pauses.next;
        let pending = pauses.devices.entry(id.to_owned()).or_insert(Pending {
            wanted: paused,
            ticket,
            writing: None,
            saved: None,
            failed: None,
            ended: None,
        });
        pending.wanted = paused;
        pending.ticket = ticket;
        if pending.idle() {
            self.write_pause(id, cx);
        }
        cx.notify();
        Some(ticket)
    }

    /// How request `ticket` for `id` ended, or `None` while it is pending.
    /// A request a later one replaced ends as that one does.
    pub(crate) fn pause_outcome(&self, id: &str, ticket: Ticket) -> Option<Outcome> {
        let Some(pending) = self.menu.pauses.devices.get(id) else {
            // Forgotten only once settled; nothing failed in time to tell.
            return Some(Outcome::Saved);
        };
        if let Some((failed, error)) = &pending.failed
            && *failed >= ticket
            && pending.saved.is_none_or(|saved| saved < *failed)
        {
            return Some(Outcome::Failed(error.clone()));
        }
        pending
            .saved
            .is_some_and(|saved| saved >= ticket)
            .then_some(Outcome::Saved)
    }

    /// Starts writing the latest request for `id`.
    fn write_pause(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(pausable) = self.pausable(id) else {
            self.pause_written(id, None, Err(fl!("host-menu-pause-gone")), cx);
            return;
        };
        let Some(pending) = self.menu.pauses.devices.get_mut(id) else {
            return;
        };
        let (paused, ticket) = (pending.wanted, pending.ticket);
        pending.writing = Some(ticket);
        tracing::info!(category = "device_pause", id, paused, "Pausing device");
        let background = cx.background_executor().spawn(async move {
            match pausable {
                Pausable::Ssh(profile) => setup::set_enabled(&profile, !paused),
                #[cfg(feature = "cloud")]
                Pausable::Cloud { provider, id } => {
                    crate::cloud::set_enabled(provider, &id, !paused).map_err(Into::into)
                }
            }
        });
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let result = background.await.map_err(|error| error.to_string());
            let _ = this.update(cx, |this, cx| {
                this.pause_written(&id, Some(ticket), result, cx);
            });
        })
        .detach();
    }

    /// Records a finished write, then writes a request made meanwhile.
    fn pause_written(
        &mut self,
        id: &str,
        ticket: Option<Ticket>,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.menu.pauses.devices.get_mut(id) else {
            return;
        };
        let ticket = ticket.unwrap_or(pending.ticket);
        pending.writing = None;
        pending.ended = Some(Instant::now());
        match result {
            Ok(()) => pending.saved = Some(ticket),
            Err(error) => {
                tracing::warn!(category = "device_pause", id, "Device pause failed");
                let label = self
                    .endpoints
                    .iter()
                    .find(|endpoint| endpoint.id == id)
                    .map(|endpoint| endpoint.label.clone())
                    .unwrap_or_default();
                pending.failed = Some((ticket, error.clone()));
                self.show_flash(
                    Flash::warning(fl!("host-menu-pause-failed", device = label, error = error)),
                    cx,
                );
            }
        }
        let newer = self
            .menu
            .pauses
            .devices
            .get(id)
            .is_some_and(|pending| pending.ticket > ticket);
        if newer {
            self.write_pause(id, cx);
        } else {
            self.catalog.refresh();
        }
        cx.notify();
    }

    /// Forgets finished requests the catalog now shows, or that ended long
    /// enough ago that it never will.
    pub(crate) fn prune_device_pauses(&mut self) {
        let endpoints = &self.endpoints;
        self.menu.pauses.devices.retain(|id, pending| {
            if !pending.idle() {
                return true;
            }
            let shown = endpoints
                .iter()
                .any(|endpoint| &endpoint.id == id && endpoint.enabled != pending.wanted);
            let old = pending.ended.is_none_or(|ended| ended.elapsed() >= SETTLE);
            !(old || (shown && !pending.latest_failed()))
        });
    }
}

#[cfg(test)]
mod tests;
