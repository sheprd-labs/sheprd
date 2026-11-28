//! Bridge between `SheprdWindow` and `InteractionStore`: ingests newly arrived
//! chat interactions, re-evaluates Smart Escalation presence, focuses source
//! chat panes, sends user replies from modal choices/runbooks/diffs, and
//! locates the originating workspace root for the secret-safe vault.

use super::{ChatEntrySync, InteractionStore, ParsedInteraction, Presence, sync_hud_deferred};
use crate::{
    NavigationTarget, SheprdWindow,
    chat::prepared::{Body, Part},
};
use gpui::{Context, Window};
use sheprd_chat_protocol::{Command, EntryId};
use sheprd_control_protocol::protocol::modal::CueKind;
use std::{path::PathBuf, time::Instant};

struct PendingPane {
    pane_id: String,
    snapshot_gen: u64,
    agent: String,
    workspace_label: String,
    workspace_root: Option<PathBuf>,
    entries: Vec<(EntryId, u64, Vec<ParsedInteraction>, Vec<CueKind>)>,
}

impl SheprdWindow {
    /// Syncs newly arrived interaction directives & cues from every live chat
    /// pane in this window into [`InteractionStore`] and refreshes presence
    /// escalation for active modals.
    pub(crate) fn sync_chat_interactions(&mut self, window: &Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let win_id = window.window_handle().window_id();
        let volume = self.config.sound.volume;
        let window_modals = self.config.chat.agent_modals;
        let global_modals = self.config.chat.global_modals;
        let progress_pill = self.config.chat.progress_pill;

        let mut pending = Vec::new();
        for pane in self.chats.panes() {
            let pane_id = pane.target.pane_id.clone();
            let snapshot_gen = pane.session.snapshots();
            let agent = pane.session.transcript().agent.clone();
            let (workspace_label, workspace_root) = self.workspace_info_for_pane(&pane_id);
            let mut entries = Vec::new();
            for (entry_id, revision) in pane.session.keys() {
                let Some(body) = pane.prepared.body(entry_id) else {
                    continue;
                };
                let (interactions, cues) = extract_interactions(&body);
                entries.push((entry_id, revision, interactions, cues));
            }
            pending.push(PendingPane {
                pane_id,
                snapshot_gen,
                agent,
                workspace_label,
                workspace_root,
                entries,
            });
        }

        let focused_pane = self
            .live
            .snapshot
            .as_deref()
            .and_then(|s| s.focused_pane_id.clone());
        let active_window = self.active;

        let Some(store) = (if pending.is_empty() {
            InteractionStore::try_global(cx)
        } else {
            Some(InteractionStore::global(cx))
        }) else {
            return;
        };

        let mut any_changed = false;
        store.update(cx, |s, cx| {
            for pane in pending {
                let is_history = s.begin_pane_sync(&pane.pane_id, pane.snapshot_gen);
                let presence =
                    if active_window && focused_pane.as_deref() == Some(pane.pane_id.as_str()) {
                        Presence::InChat
                    } else if active_window {
                        Presence::SameWindowOtherTab
                    } else {
                        Presence::OtherApp
                    };
                for (entry_id, revision, interactions, cues) in pane.entries {
                    any_changed |= s.sync_chat_entry(ChatEntrySync {
                        window_id: win_id,
                        pane_id: &pane.pane_id,
                        agent_label: &pane.agent,
                        workspace_label: &pane.workspace_label,
                        workspace_root: pane.workspace_root.clone(),
                        entry_id,
                        revision,
                        history: is_history,
                        presence,
                        interactions: &interactions,
                        cues: &cues,
                        now,
                        volume,
                        window_modals_enabled: window_modals,
                        global_modals_enabled: global_modals,
                        progress_pill_enabled: progress_pill,
                    });
                }
            }
            any_changed |= s.refresh_presence(
                win_id,
                active_window,
                focused_pane.as_deref(),
                global_modals,
                now,
            );
            let needs_hud = !s.hud_open && !s.global_hud_items(now).is_empty();
            if any_changed || needs_hud {
                sync_hud_deferred(cx);
                cx.notify();
            }
        });
        if any_changed {
            cx.notify();
        }
    }

    /// Focuses the chat pane that originated an interaction.
    pub(crate) fn focus_chat_pane_by_id(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        let target = NavigationTarget::Pane(pane_id);
        let _ = self.navigate(target, cx);
    }

    /// Sends a user response (choice click, runbook completion, diff approval,
    /// or redacted vault receipt) back into the originating chat pane.
    pub(crate) fn send_interaction_reply(
        &mut self,
        pane_id: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.chats.pane_mut(pane_id) {
            let origin = Some(pane.target.origin());
            let _ = pane.send(Command::Prompt {
                attachments: Vec::new(),
                origin,
                text: text.to_owned(),
            });
            cx.notify();
        }
    }

    /// Label and root directory of the workspace that owns `pane_id`.
    pub(crate) fn workspace_info_for_pane(&self, pane_id: &str) -> (String, Option<PathBuf>) {
        self.live
            .snapshot
            .as_deref()
            .and_then(|s| {
                let pane = s.panes.iter().find(|p| p.pane_id == pane_id)?;
                let ws = s
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == pane.workspace_id)?;
                let root = (!ws.new_workspace_cwd.is_empty())
                    .then(|| PathBuf::from(&ws.new_workspace_cwd));
                Some((ws.label.clone(), root))
            })
            .unwrap_or_else(|| ("workspace".to_owned(), None))
    }

    /// Root directory of the workspace that owns `pane_id`.
    pub(crate) fn workspace_path_for_pane(&self, pane_id: &str) -> Option<PathBuf> {
        self.workspace_info_for_pane(pane_id).1
    }

    /// Root directory of `workspace_id`, when shown in this window.
    pub(crate) fn workspace_path_for_id(&self, workspace_id: &str) -> Option<PathBuf> {
        let snapshot = self.live.snapshot.as_deref()?;
        snapshot
            .workspaces
            .iter()
            .find(|w| w.workspace_id == workspace_id)
            .and_then(|w| {
                (!w.new_workspace_cwd.is_empty()).then(|| PathBuf::from(&w.new_workspace_cwd))
            })
    }

    /// Root directory of the active workspace in this window, used only when a
    /// CLI caller did not name a specific workspace or pane.
    pub(crate) fn selected_workspace_path(&self) -> Option<PathBuf> {
        let snapshot = self.live.snapshot.as_deref()?;
        let focused_ws = snapshot.focused_workspace_id.as_deref()?;
        self.workspace_path_for_id(focused_ws)
    }
}

fn extract_interactions(body: &Body) -> (Vec<ParsedInteraction>, Vec<CueKind>) {
    let Body::Message(parts) = body else {
        return (Vec::new(), Vec::new());
    };
    let mut interactions = Vec::new();
    let mut cues = Vec::new();
    for part in parts {
        match part {
            Part::Interaction(parsed) => interactions.push((**parsed).clone()),
            Part::Cue(cue) => cues.push(*cue),
            _ => {}
        }
    }
    (interactions, cues)
}
