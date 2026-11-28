//! Which chats keep their place when a window has more than it attaches.
//!
//! The bound ([`crate::chat::discovery::bounded`]) attaches the panes on
//! screen first, then the ones the person saw most recently ([`Seen`]), so
//! the chat just left is not the one let go. A chat let go only for room
//! rests ([`Resting`]) with its transcript, list, and composer, but no
//! connection: coming back to it shows what it had at once, as a dropped
//! connection does, while it reconnects. Without this, going back to such
//! a chat showed its terminal until a new connection brought the whole
//! transcript again, which took seconds through a relay.

use super::ChatPane;
use crate::chat::{Error, Target};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Chats kept resting at once, beyond the attached ones. Each holds a
/// transcript its program bounds, and no connection or thread.
pub(super) const MAX_RESTING: usize = 16;
/// Panes remembered as seen. Well above the chats a window attaches, so the
/// order holds however many the person moves between.
const MAX_SEEN: usize = 64;

/// Chats let go for room, oldest first.
#[derive(Default)]
pub(super) struct Resting {
    panes: VecDeque<(ChatPane, Instant)>,
}

/// What became of a chat set aside.
pub(super) enum Put {
    /// It rests, and pushed out the oldest, which is gone now.
    Kept(Option<ChatPane>),
    /// It had nothing to show yet, or cannot reconnect: it is gone.
    Refused(ChatPane),
}

impl Resting {
    /// Keeps `pane` without its connection, so it shows again at once.
    pub(super) fn put(&mut self, mut pane: ChatPane, now: Instant) -> Put {
        if !pane.available() {
            return Put::Refused(pane);
        }
        pane.rest();
        self.panes.retain(|(kept, _)| kept.target != pane.target);
        self.panes.push_back((pane, now));
        let out = (self.panes.len() > MAX_RESTING)
            .then(|| self.panes.pop_front().map(|(pane, _)| pane))
            .flatten();
        Put::Kept(out)
    }

    /// The resting chat of `target`, and how long it rested.
    pub(super) fn take(&mut self, target: &Target, now: Instant) -> Option<(ChatPane, Duration)> {
        let index = self
            .panes
            .iter()
            .position(|(pane, _)| pane.target == *target)?;
        let (pane, since) = self.panes.remove(index)?;
        Some((pane, now.saturating_duration_since(since)))
    }

    /// Forgets the chats whose program is no longer running.
    pub(super) fn retain(&mut self, known: &[Target]) {
        self.panes.retain(|(pane, _)| known.contains(&pane.target));
    }

    pub(super) fn len(&self) -> usize {
        self.panes.len()
    }

    #[cfg(test)]
    pub(super) fn contains(&self, pane_id: &str) -> bool {
        self.panes
            .iter()
            .any(|(pane, _)| pane.target.pane_id == pane_id)
    }
}

impl super::Chats {
    /// Sets aside `pane`, which the bound let go though its program still
    /// runs. One that cannot rest, or the oldest one it pushes out, gives
    /// its unsent files to [`super::Chats::parked`] instead.
    pub(super) fn set_aside(&mut self, pane: ChatPane, now: Instant) {
        let pane_id = pane.target.pane_id.clone();
        let transport = pane.target.route.transport_label();
        let gone = match self.resting.put(pane, now) {
            Put::Kept(out) => {
                tracing::info!(
                    pane = %pane_id,
                    transport,
                    resting = self.resting.len(),
                    "Chat set aside to make room"
                );
                out
            }
            Put::Refused(pane) => {
                tracing::debug!(pane = %pane_id, transport, "Chat let go to make room");
                Some(pane)
            }
        };
        if let Some(mut gone) = gone
            && !gone.attachments.is_empty()
        {
            self.parked
                .insert(gone.target.clone(), std::mem::take(&mut gone.attachments));
        }
    }

    /// The pane for `target`, which was not attached: its resting chat,
    /// shown at once, or a new one holding the files kept for it.
    /// Whether the chat of `pane_id` rests.
    #[cfg(test)]
    pub(crate) fn resting(&self, pane_id: &str) -> bool {
        self.resting.contains(pane_id)
    }

    pub(super) fn revive(&mut self, target: Target, now: Instant) -> ChatPane {
        if let Some((pane, rested)) = self.resting.take(&target, now) {
            // Layout, width, and code settings may have changed meanwhile;
            // the snapshot on its way prepares the text again.
            pane.list.remeasure();
            tracing::info!(
                pane = %pane.target.pane_id,
                transport = pane.target.route.transport_label(),
                rested_ms = u64::try_from(rested.as_millis()).unwrap_or(u64::MAX),
                "Chat back from rest, shown while it reconnects"
            );
            return pane;
        }
        tracing::debug!(
            pane = %target.pane_id,
            transport = target.route.transport_label(),
            "Chat attaching without a transcript"
        );
        let mut pane = ChatPane::new(target);
        if let Some(files) = self.parked.remove(&pane.target) {
            pane.attachments = files;
        }
        pane
    }
}

impl ChatPane {
    /// Lets the connection go and keeps the rest. The next connection asks
    /// for a whole snapshot, as one that fell behind does, rather than a
    /// replay of everything missed meanwhile; what it had stays on screen.
    fn rest(&mut self) {
        self.link = None;
        self.retry_at = None;
        self.failures = 0;
        self.dialed = None;
        self.menu = None;
        self.menu_search = None;
        self.session.lost(&Error::Overflow);
        // Workers still running for it finish into a pane no longer
        // attached, so their tickets would never come back and would hold
        // its worker slots. A new generation drops their results instead;
        // the textures were already released when it was set aside.
        self.prepared.visuals = Default::default();
    }
}

/// The panes on screen, most recently seen first.
#[derive(Default)]
pub(crate) struct Seen {
    panes: VecDeque<String>,
}

impl Seen {
    /// Notes `panes`, the ones on screen with the focused first, as the
    /// most recently seen.
    pub(crate) fn note<'a>(&mut self, panes: impl IntoIterator<Item = &'a str>) {
        let mut unique: Vec<&str> = Vec::new();
        for pane in panes {
            if !unique.contains(&pane) {
                unique.push(pane);
            }
        }
        let panes = unique;
        // Every tick notes the same panes; only a change moves the order.
        if self.panes.len() >= panes.len()
            && panes
                .iter()
                .zip(&self.panes)
                .all(|(pane, seen)| *pane == seen)
        {
            return;
        }
        self.panes.retain(|seen| !panes.contains(&seen.as_str()));
        for pane in panes.into_iter().rev() {
            self.panes.push_front(pane.to_owned());
        }
        self.panes.truncate(MAX_SEEN);
    }

    /// How recently `pane` was seen: `0` for the latest.
    fn recency(&self, pane: &str) -> Option<usize> {
        self.panes.iter().position(|seen| seen == pane)
    }

    /// Where the bound puts `pane`, lower first: the `focused` pane on
    /// screen, the others `on_screen`, those `holding` files for an unsent
    /// prompt, then the rest by how recently they were seen, and last those
    /// never seen.
    pub(crate) fn rank(
        &self,
        pane: &str,
        focused: Option<&str>,
        on_screen: &[String],
        holding: &[String],
    ) -> u8 {
        if focused == Some(pane) {
            0
        } else if on_screen.iter().any(|shown| shown == pane) {
            1
        } else if holding.iter().any(|held| held == pane) {
            2
        } else {
            self.recency(pane).map_or(u8::MAX, |at| {
                u8::try_from(at).map_or(u8::MAX - 1, |at| at.saturating_add(3).min(u8::MAX - 1))
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
