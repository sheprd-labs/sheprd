//! Persistent agent interactions: window modals, global always-on-top HUD,
//! dynamic progress pill, secret-safe vault, shared runbooks, drop-zones,
//! spotlights, mini-diffs, smart escalation, and semantic cues.

pub(crate) mod audio;
pub(crate) mod error;
pub(crate) mod hud_window;
pub(crate) mod parse;
pub(crate) mod render_window;
pub(crate) mod vault;
mod window_bridge;

#[cfg(test)]
mod tests;

pub(crate) use parse::{BlockDirective, ParsedInteraction, Payload};

use crate::window::main_window::MainWindow;
use gpui::{App, AppContext, Context, Entity, Global, WindowId};
use sheprd_chat_protocol::EntryId;
use sheprd_control_protocol::protocol::modal::{CueKind, ModalScope, ModalSummary, RunbookStep};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

/// Maximum concurrent active interactions retained across all chats/windows.
pub(crate) const MAX_ACTIVE: usize = 16;
/// How long an explicit Edge Light cue override pulses.
const CUE_DURATION: Duration = Duration::from_secs(4);
/// Duration of a green "Done ✓" flash before auto-clearing a progress pill.
const DONE_FLASH: Duration = Duration::from_secs(3);

/// Where the user currently is, used by the Smart Escalation Ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    /// Looking directly at the chat pane that emitted this interaction.
    InChat,
    /// Active inside the same Sheprd window, on a different tab/workspace.
    SameWindowOtherTab,
    /// Sheprd is not frontmost (or user is on another Space/app).
    OtherApp,
    /// User has been idle for >= `idle_secs` seconds (escalate to paired phone).
    #[allow(dead_code)]
    Away { idle_secs: u64 },
}

/// Surface selected by the Smart Escalation Ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EscalationLevel {
    /// Quiet inline chip in the active chat only.
    InlineOnly,
    /// Persistent modal/sheet over the chat's Sheprd window.
    WindowModal,
    /// Always-on-top global floating HUD across all apps & Spaces + Dock bounce.
    GlobalHud,
    /// Global HUD + paired iPhone Live Activity / time-sensitive push.
    PhoneEscalation,
}

/// Pure routing function for the Smart Escalation Ladder.
pub(crate) fn escalate(
    scope: ModalScope,
    presence: Presence,
    unread_secs: u64,
    global_enabled: bool,
) -> EscalationLevel {
    match presence {
        Presence::InChat => EscalationLevel::InlineOnly,
        Presence::SameWindowOtherTab => EscalationLevel::WindowModal,
        Presence::OtherApp => {
            if global_enabled && (scope == ModalScope::All || unread_secs >= 10) {
                EscalationLevel::GlobalHud
            } else {
                EscalationLevel::WindowModal
            }
        }
        Presence::Away { idle_secs } => {
            if global_enabled && idle_secs >= 20 {
                EscalationLevel::PhoneEscalation
            } else if global_enabled && scope == ModalScope::All {
                EscalationLevel::GlobalHud
            } else {
                EscalationLevel::WindowModal
            }
        }
    }
}

/// Origin of an active interaction: a chat pane or an explicit CLI call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SourceKey {
    pub(crate) window_id: Option<WindowId>,
    pub(crate) pane_id: Option<String>,
}

/// Parameters for inserting or replacing an active interaction in [`InteractionStore`].
pub(crate) struct UpsertInteraction {
    pub(crate) source: SourceKey,
    pub(crate) agent_label: String,
    pub(crate) workspace_label: String,
    pub(crate) workspace_root: Option<PathBuf>,
    pub(crate) entry_id: Option<EntryId>,
    pub(crate) source_revision: Option<u64>,
    pub(crate) directive_index: Option<usize>,
    pub(crate) parsed: ParsedInteraction,
    pub(crate) display_level: EscalationLevel,
    pub(crate) manual_open: bool,
    pub(crate) now: Instant,
    pub(crate) volume: u8,
}

/// Parameters for syncing a single chat entry into [`InteractionStore`].
pub(crate) struct ChatEntrySync<'a> {
    pub(crate) window_id: WindowId,
    pub(crate) pane_id: &'a str,
    pub(crate) agent_label: &'a str,
    pub(crate) workspace_label: &'a str,
    pub(crate) workspace_root: Option<PathBuf>,
    pub(crate) entry_id: EntryId,
    pub(crate) revision: u64,
    pub(crate) history: bool,
    pub(crate) presence: Presence,
    pub(crate) interactions: &'a [ParsedInteraction],
    pub(crate) cues: &'a [CueKind],
    pub(crate) now: Instant,
    pub(crate) volume: u8,
    pub(crate) window_modals_enabled: bool,
    pub(crate) global_modals_enabled: bool,
    pub(crate) progress_pill_enabled: bool,
}

/// Visibility state of a chat entry's interaction card, used by the inline chat
/// chip so focused-chat `InlineOnly` requests offer an immediate "Open" control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntryModalState {
    None,
    InlineOnly(String),
    Open(String),
}

/// One live persistent interaction tracked by [`InteractionStore`].
#[derive(Clone, Debug)]
pub(crate) struct ActiveInteraction {
    pub(crate) id: String,
    pub(crate) source: SourceKey,
    pub(crate) agent_label: String,
    pub(crate) workspace_label: String,
    /// Root directory of the originating workspace (for secret-safe `.env` writes).
    pub(crate) workspace_root: Option<PathBuf>,
    pub(crate) entry_id: Option<EntryId>,
    /// Last transcript entry revision applied to `parsed`, so unchanged polls
    /// never overwrite locally toggled runbook checkboxes.
    pub(crate) source_revision: Option<u64>,
    /// Ordinal index of the directive within `entry_id`, so streaming chunks
    /// update the active card in-place without reopening dismissed cards.
    pub(crate) directive_index: Option<usize>,
    pub(crate) parsed: ParsedInteraction,
    /// Current surface chosen by the Smart Escalation Ladder (or manual open).
    pub(crate) display_level: EscalationLevel,
    /// Explicitly opened via CLI or user clicking "Show" on a chat chip.
    pub(crate) manual_open: bool,
    pub(crate) created_at: Instant,
    pub(crate) snoozed_until: Option<Instant>,
    pub(crate) expires_at: Option<Instant>,
    /// Transient status line shown after a user clicks an action or vault submit.
    pub(crate) feedback: Option<String>,
}

impl ActiveInteraction {
    pub(crate) fn is_progress(&self) -> bool {
        matches!(self.parsed.payload, Payload::Progress { .. })
    }

    pub(crate) fn is_visible(&self, now: Instant) -> bool {
        if self.expires_at.is_some_and(|exp| now >= exp) {
            return false;
        }
        self.snoozed_until.is_none_or(|until| now >= until)
    }

    pub(crate) fn summary(&self) -> ModalSummary {
        let percent = match &self.parsed.payload {
            Payload::Progress { percent, .. } => *percent,
            _ => None,
        };
        ModalSummary {
            id: self.id.clone(),
            kind: self.parsed.directive.kind_name().to_owned(),
            scope: self.parsed.scope,
            title: self.parsed.title.clone(),
            source: Some(format!("{} · {}", self.agent_label, self.workspace_label)),
            percent,
        }
    }
}

/// Temporary Edge Light cue override active on windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EdgeCue {
    pub(crate) cue: CueKind,
    pub(crate) until: Instant,
}

/// How many directives and standalone cues have already been triggered for a
/// chat entry, so streaming chunks update in-place without replaying cues or
/// reopening dismissed modals.
#[derive(Clone, Copy, Debug, Default)]
struct SeenEntry {
    interactions: usize,
    cues: usize,
}

/// App-global store of all active persistent modals, progress pills, runbooks,
/// spotlights, and semantic cues across windows and the floating global HUD.
#[derive(Default)]
pub(crate) struct InteractionStore {
    next_id: u64,
    items: Vec<ActiveInteraction>,
    /// Last observed `session.snapshots()` generation per pane so initial
    /// transcript loads and snapshot reloads are always treated as history.
    pane_snapshots: HashMap<String, u64>,
    /// Directives and cues already ingested per `(pane_id, entry_id)`.
    seen_entries: HashMap<String, HashMap<EntryId, SeenEntry>>,
    /// Currently selected card index inside the stacked deck.
    pub(crate) deck_index: usize,
    /// Whether the global Dynamic Progress Pill is expanded on hover/click.
    pub(crate) pill_expanded: bool,
    /// Last explicit audio/Edge Light cue fired.
    edge_cue: Option<EdgeCue>,
    /// Handle to the floating always-on-top HUD window, if open.
    pub(crate) hud_open: bool,
}

/// Global GPUI wrapper holding the shared `Entity<InteractionStore>`.
pub(crate) struct GlobalInteractions(pub(crate) Entity<InteractionStore>);

impl Global for GlobalInteractions {}

impl InteractionStore {
    /// Returns the shared app-wide store, initializing it on first use.
    pub(crate) fn global(cx: &mut App) -> Entity<Self> {
        if let Some(existing) = cx.try_global::<GlobalInteractions>() {
            return existing.0.clone();
        }
        let entity = cx.new(|_| Self::default());
        cx.set_global(GlobalInteractions(entity.clone()));
        entity
    }

    /// Reads the shared store if initialized.
    pub(crate) fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalInteractions>().map(|g| g.0.clone())
    }

    /// Records `snapshot_gen` for `pane_id`. Returns `true` when the pane is
    /// newly observed or loaded a fresh transcript snapshot, meaning all
    /// entries currently in the transcript must be marked as history without
    /// firing modals or cues.
    pub(crate) fn begin_pane_sync(&mut self, pane_id: &str, snapshot_gen: u64) -> bool {
        let is_history = match self.pane_snapshots.get(pane_id) {
            None => true,
            Some(&prev) => prev != snapshot_gen,
        };
        if is_history {
            self.pane_snapshots.insert(pane_id.to_owned(), snapshot_gen);
            self.seen_entries
                .entry(pane_id.to_owned())
                .or_default()
                .clear();
        }
        is_history
    }

    /// Inserts or replaces the active interaction of the same category (modal
    /// vs progress pill) for `source`, returning its assigned ID.
    pub(crate) fn upsert(&mut self, req: UpsertInteraction) -> String {
        self.prune_expired(req.now);
        let is_progress = matches!(req.parsed.payload, Payload::Progress { .. });
        let done_flash = matches!(req.parsed.payload, Payload::Progress { done: true, .. });
        // At most one modal and one progress pill per source pane so agents
        // can update in place without flooding the user's screen.
        if req.source.pane_id.is_some() {
            self.items.retain(|item| {
                !(item.source.pane_id == req.source.pane_id && item.is_progress() == is_progress)
            });
        }
        if let Some(cue) = req.parsed.cue {
            self.trigger_cue(cue, req.now, req.volume);
        }
        self.next_id += 1;
        let id = format!("m{}", self.next_id);
        let expires_at = if done_flash {
            Some(req.now + DONE_FLASH)
        } else {
            None
        };
        self.items.push(ActiveInteraction {
            id: id.clone(),
            source: req.source,
            agent_label: req.agent_label,
            workspace_label: req.workspace_label,
            workspace_root: req.workspace_root,
            entry_id: req.entry_id,
            source_revision: req.source_revision,
            directive_index: req.directive_index,
            parsed: req.parsed,
            display_level: req.display_level,
            manual_open: req.manual_open,
            created_at: req.now,
            snoozed_until: None,
            expires_at,
            feedback: None,
        });
        while self.items.len() > MAX_ACTIVE {
            self.items.remove(0);
        }
        if self.deck_index >= self.items.len() {
            self.deck_index = self.items.len().saturating_sub(1);
        }
        id
    }

    /// Records a semantic cue (audio + Edge Light override).
    pub(crate) fn trigger_cue(&mut self, cue: CueKind, now: Instant, volume: u8) {
        self.edge_cue = Some(EdgeCue {
            cue,
            until: now + CUE_DURATION,
        });
        audio::play_async(cue, volume);
    }

    /// Active Edge Light cue override at `now`, if any.
    pub(crate) fn active_edge_cue(&self, now: Instant) -> Option<CueKind> {
        self.edge_cue
            .filter(|edge| now < edge.until)
            .map(|edge| edge.cue)
    }

    /// Ingests newly arrived chat interactions from `sync.pane_id`. Historical
    /// transcript loads are recorded as seen without popping modals; streaming
    /// chunks update active cards in-place without replaying cues or reopening
    /// dismissed modals; a newer agent entry auto-clears prior `auto_clear`
    /// modals for that pane.
    pub(crate) fn sync_chat_entry(&mut self, sync: ChatEntrySync<'_>) -> bool {
        let pane_seen = self
            .seen_entries
            .entry(sync.pane_id.to_owned())
            .or_default();
        if sync.history {
            let entry = pane_seen.entry(sync.entry_id).or_default();
            entry.interactions = entry.interactions.max(sync.interactions.len());
            entry.cues = entry.cues.max(sync.cues.len());
            return false;
        }
        let first_time = !pane_seen.contains_key(&sync.entry_id);
        let prev_seen = pane_seen.get(&sync.entry_id).copied().unwrap_or_default();
        let entry_slot = pane_seen.entry(sync.entry_id).or_default();
        entry_slot.interactions = entry_slot.interactions.max(sync.interactions.len());
        entry_slot.cues = entry_slot.cues.max(sync.cues.len());

        let mut changed = false;

        // A brand-new entry from the same chat auto-clears earlier `auto_clear`
        // modals (e.g., git push succeeded after YubiKey touch).
        if first_time {
            let before = self.items.len();
            self.items.retain(|item| {
                !(item.source.pane_id.as_deref() == Some(sync.pane_id)
                    && item.parsed.auto_clear
                    && item.entry_id.is_some_and(|prev| prev < sync.entry_id))
            });
            changed |= self.items.len() != before;
        }

        // Only play standalone `::cue` directives that were not already played
        // in an earlier streamed revision of this entry.
        if sync.cues.len() > prev_seen.cues {
            for &cue in &sync.cues[prev_seen.cues..] {
                self.trigger_cue(cue, sync.now, sync.volume);
                changed = true;
            }
        }

        // Streamed updates to already-ingested directives update active items
        // in-place (preserving created_at, snooze, feedback, and locally
        // completed runbook steps) only when the transcript revision actually
        // advanced, and never reopen dismissed items or replay cues.
        let update_limit = prev_seen.interactions.min(sync.interactions.len());
        for (idx, parsed) in sync.interactions[..update_limit].iter().enumerate() {
            if let Some(existing) = self.items.iter_mut().find(|item| {
                item.source.pane_id.as_deref() == Some(sync.pane_id)
                    && item.entry_id == Some(sync.entry_id)
                    && item.directive_index == Some(idx)
            }) {
                if existing.source_revision == Some(sync.revision) {
                    continue;
                }
                existing.source_revision = Some(sync.revision);
                let mut updated = parsed.clone();
                if !matches!(updated.payload, Payload::Progress { .. })
                    && updated.scope == ModalScope::All
                    && !sync.global_modals_enabled
                {
                    updated.scope = ModalScope::Window;
                }
                preserve_completed_runbook_steps(&existing.parsed, &mut updated);
                if existing.parsed != updated {
                    let was_done = matches!(
                        existing.parsed.payload,
                        Payload::Progress { done: true, .. }
                    );
                    let now_done = matches!(updated.payload, Payload::Progress { done: true, .. });
                    existing.parsed = updated;
                    if sync.workspace_root.is_some() {
                        existing.workspace_root.clone_from(&sync.workspace_root);
                    }
                    if !was_done && now_done {
                        existing.expires_at = Some(sync.now + DONE_FLASH);
                        self.trigger_cue(CueKind::Done, sync.now, sync.volume);
                    }
                    changed = true;
                }
            }
        }

        // Newly arrived interaction blocks in this entry.
        if sync.interactions.len() > prev_seen.interactions {
            for (offset, parsed) in sync.interactions[prev_seen.interactions..]
                .iter()
                .enumerate()
            {
                let idx = prev_seen.interactions + offset;
                let is_progress = matches!(parsed.payload, Payload::Progress { .. });
                if is_progress && !sync.progress_pill_enabled {
                    continue;
                }
                if !is_progress && parsed.scope == ModalScope::Window && !sync.window_modals_enabled
                {
                    continue;
                }
                let mut parsed = parsed.clone();
                if !is_progress && parsed.scope == ModalScope::All && !sync.global_modals_enabled {
                    if !sync.window_modals_enabled {
                        continue;
                    }
                    parsed.scope = ModalScope::Window;
                }
                let display_level = if is_progress {
                    EscalationLevel::GlobalHud
                } else {
                    escalate(parsed.scope, sync.presence, 0, sync.global_modals_enabled)
                };
                self.upsert(UpsertInteraction {
                    source: SourceKey {
                        window_id: Some(sync.window_id),
                        pane_id: Some(sync.pane_id.to_owned()),
                    },
                    agent_label: sync.agent_label.to_owned(),
                    workspace_label: sync.workspace_label.to_owned(),
                    workspace_root: sync.workspace_root.clone(),
                    entry_id: Some(sync.entry_id),
                    source_revision: Some(sync.revision),
                    directive_index: Some(idx),
                    parsed,
                    display_level,
                    manual_open: false,
                    now: sync.now,
                    volume: sync.volume,
                });
                changed = true;
            }
        }
        changed
    }

    /// Re-evaluates `display_level` for all active chat-originated items in
    /// `window_id` from the user's current presence and elapsed unread time.
    /// Returns `true` if any item's surface changed or expired.
    pub(crate) fn refresh_presence(
        &mut self,
        window_id: WindowId,
        active_window: bool,
        focused_pane: Option<&str>,
        global_modals_enabled: bool,
        now: Instant,
    ) -> bool {
        let before_len = self.items.len();
        self.prune_expired(now);
        let mut changed = self.items.len() != before_len;
        for item in &mut self.items {
            if item.manual_open || item.is_progress() || item.source.window_id != Some(window_id) {
                continue;
            }
            let presence = if active_window
                && focused_pane.is_some()
                && item.source.pane_id.as_deref() == focused_pane
            {
                Presence::InChat
            } else if active_window {
                Presence::SameWindowOtherTab
            } else {
                Presence::OtherApp
            };
            let unread_secs = now.saturating_duration_since(item.created_at).as_secs();
            let next = escalate(
                item.parsed.scope,
                presence,
                unread_secs,
                global_modals_enabled,
            );
            if item.display_level != next {
                item.display_level = next;
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn dismiss(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        if self.deck_index >= self.items.len() {
            self.deck_index = self.items.len().saturating_sub(1);
        }
        self.items.len() != before
    }

    pub(crate) fn snooze(&mut self, id: &str, duration: Duration, now: Instant) -> bool {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.snoozed_until = Some(now + duration);
            return true;
        }
        false
    }

    /// Snoozes `id` for `duration` and schedules a background timer to wake up
    /// the global HUD and all Sheprd windows when the snooze expires.
    pub(crate) fn snooze_and_schedule(
        &mut self,
        id: &str,
        duration: Duration,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.snooze(id, duration, now) {
            return false;
        }
        let wake_after = duration.saturating_add(Duration::from_millis(50));
        cx.spawn(async move |store, cx| {
            cx.background_executor().timer(wake_after).await;
            cx.update(|cx| {
                if let Some(store) = store.upgrade() {
                    store.update(cx, |_, cx| cx.notify());
                    Self::ensure_hud(&store, cx);
                }
                for win in MainWindow::all(cx) {
                    let _ = win.update(cx, |_, _, cx| cx.notify());
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(crate) fn toggle_runbook_step(&mut self, id: &str, index: usize) -> Option<RunbookStep> {
        let item = self.items.iter_mut().find(|item| item.id == id)?;
        let Payload::Runbook { steps } = &mut item.parsed.payload else {
            return None;
        };
        let step = steps.get_mut(index)?;
        step.done = !step.done;
        Some(step.clone())
    }

    pub(crate) fn set_feedback(&mut self, id: &str, text: String) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.feedback = Some(text);
        }
    }

    pub(crate) fn clear_for_pane(&mut self, pane_id: &str) -> usize {
        let before = self.items.len();
        self.items
            .retain(|item| item.source.pane_id.as_deref() != Some(pane_id));
        before - self.items.len()
    }

    pub(crate) fn clear_all(&mut self) -> usize {
        let count = self.items.len();
        self.items.clear();
        self.deck_index = 0;
        count
    }

    pub(crate) fn prune_expired(&mut self, now: Instant) {
        self.items
            .retain(|item| item.expires_at.is_none_or(|exp| now < exp));
    }

    /// Visible items belonging to `window_id` (plus mirrored global items)
    /// whose current escalation level is `WindowModal` or higher.
    pub(crate) fn for_window(&self, window_id: WindowId, now: Instant) -> Vec<&ActiveInteraction> {
        self.items
            .iter()
            .filter(|item| {
                item.is_visible(now)
                    && !item.is_progress()
                    && item.display_level != EscalationLevel::InlineOnly
                    && (matches!(
                        item.display_level,
                        EscalationLevel::GlobalHud | EscalationLevel::PhoneEscalation
                    ) || item.source.window_id == Some(window_id)
                        || item.source.window_id.is_none())
            })
            .collect()
    }

    /// Visible items escalated to the always-on-top global HUD (or active
    /// progress pills).
    pub(crate) fn global_hud_items(&self, now: Instant) -> Vec<&ActiveInteraction> {
        self.items
            .iter()
            .filter(|item| {
                item.is_visible(now)
                    && (item.is_progress()
                        || matches!(
                            item.display_level,
                            EscalationLevel::GlobalHud | EscalationLevel::PhoneEscalation
                        ))
            })
            .collect()
    }

    /// Visible progress pills across all chats.
    pub(crate) fn progress_items(&self, now: Instant) -> Vec<&ActiveInteraction> {
        self.items
            .iter()
            .filter(|item| item.is_visible(now) && item.is_progress())
            .collect()
    }

    pub(crate) fn summaries(&self, now: Instant) -> Vec<ModalSummary> {
        self.items
            .iter()
            .filter(|item| item.is_visible(now))
            .map(ActiveInteraction::summary)
            .collect()
    }

    /// Promotes an active (possibly `InlineOnly` or snoozed) card so its full
    /// action controls appear immediately without dismissing or recreating it.
    pub(crate) fn open_card(&mut self, id: &str, global_modals_enabled: bool) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.manual_open = true;
        item.snoozed_until = None;
        item.display_level = if item.is_progress()
            || (item.parsed.scope == ModalScope::All && global_modals_enabled)
        {
            EscalationLevel::GlobalHud
        } else {
            EscalationLevel::WindowModal
        };
        true
    }

    pub(crate) fn entry_modal_state(
        &self,
        pane_id: &str,
        entry_id: EntryId,
        is_progress: bool,
    ) -> EntryModalState {
        let Some(item) = self.items.iter().find(|item| {
            item.source.pane_id.as_deref() == Some(pane_id)
                && item.entry_id == Some(entry_id)
                && item.is_progress() == is_progress
        }) else {
            return EntryModalState::None;
        };
        if item.display_level == EscalationLevel::InlineOnly {
            EntryModalState::InlineOnly(item.id.clone())
        } else {
            EntryModalState::Open(item.id.clone())
        }
    }

    pub(crate) fn step_deck(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.deck_index = 0;
            return;
        }
        let next = (self.deck_index as isize + delta).rem_euclid(len as isize);
        self.deck_index = next as usize;
    }

    /// Ensures the floating global HUD window is open whenever any global
    /// modal or progress pill is active.
    pub(crate) fn ensure_hud(store: &Entity<Self>, cx: &mut App) {
        let now = Instant::now();
        let needs_hud = {
            let s = store.read(cx);
            !s.hud_open && !s.global_hud_items(now).is_empty()
        };
        if needs_hud {
            hud_window::open(store.clone(), cx);
        }
    }
}

/// Helper used by `Context<InteractionStore>` callers to notify observers and
/// bring up the global HUD when needed.
pub(crate) fn sync_hud_deferred(cx: &mut Context<InteractionStore>) {
    let entity = cx.entity();
    cx.defer(move |cx| {
        InteractionStore::ensure_hud(&entity, cx);
    });
}

/// Preserves locally completed human runbook steps when a newer streamed
/// transcript revision updates the same runbook block.
fn preserve_completed_runbook_steps(existing: &ParsedInteraction, updated: &mut ParsedInteraction) {
    let (
        Payload::Runbook {
            steps: existing_steps,
        },
        Payload::Runbook {
            steps: updated_steps,
        },
    ) = (&existing.payload, &mut updated.payload)
    else {
        return;
    };
    for (idx, step) in updated_steps.iter_mut().enumerate() {
        if step.done || !step.human {
            continue;
        }
        let locally_done = existing_steps
            .get(idx)
            .filter(|prev| prev.label == step.label)
            .or_else(|| existing_steps.iter().find(|prev| prev.label == step.label))
            .is_some_and(|prev| prev.done);
        if locally_done {
            step.done = true;
        }
    }
}
