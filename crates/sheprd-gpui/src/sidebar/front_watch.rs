//! Where the front goes when the workspace in front disappears: removed with
//! its worktree, or closed, from this window, the CLI, or a phone. Herdr 1
//! keeps the daemon's own index, which its sidebar groups differently, and a
//! kept Herdr 2 workspace falls back to the first, so a long list jumps away
//! from where the user was. Instead the row above it in the sidebar comes to
//! the front, or the row below when it was first.

use super::order::WorkspaceRow;
use crate::{
    NavigationTarget, SheprdWindow,
    menu::{Page, WorkspaceAction},
};
use gpui::Context;
use std::time::{Duration, Instant};

/// How long a successor waits for an open menu or dialog to close.
const PATIENCE: Duration = Duration::from_secs(2);

/// The workspace in front and the row that would take its place.
#[derive(Default)]
pub(crate) struct FrontWatch {
    front: Option<Front>,
    /// What `front.successor` was found against, so the rows are only read
    /// again when the front or a session changes.
    key: Option<Key>,
    /// When the front was seen gone while a menu kept its successor waiting.
    gone_at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Front {
    endpoint: usize,
    generation: u64,
    /// Herdr 1's boot, as workspace IDs do not outlive a daemon.
    boot: Option<String>,
    workspace: String,
    /// The rows that may take its place, best first.
    successors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    endpoint: usize,
    workspace: String,
    snapshot: Option<usize>,
    core: Option<usize>,
}

/// The rows that may take `workspace`'s place among `host`'s rows, best
/// first: those above it from the nearest up, then those below from the
/// nearest down. The first still listed wins, so removing several rows at
/// once lands on the nearest survivor above. Other hosts' rows never count.
pub(crate) fn successors(rows: &[WorkspaceRow], host: usize, workspace: &str) -> Vec<String> {
    let own: Vec<&str> = rows
        .iter()
        .filter(|(index, _)| *index == host)
        .map(|(_, id)| id.as_str())
        .collect();
    let Some(at) = own.iter().position(|id| *id == workspace) else {
        return Vec::new();
    };
    own[..at]
        .iter()
        .rev()
        .chain(&own[at + 1..])
        .map(|id| (*id).to_owned())
        .collect()
}

impl SheprdWindow {
    /// Brings the row above to the front once the workspace in front is
    /// gone, then follows whatever is in front now.
    pub(crate) fn follow_removed_front(&mut self, cx: &mut Context<Self>) {
        let endpoint = self.selected_endpoint;
        let generation = self.endpoints[endpoint].generation;
        let boot = self.live.snapshot.as_ref().map(|s| s.boot_id.clone());
        let current = self.browser_key().map(|(_, id)| id);
        if let Some(front) = self.front_watch.front.clone()
            && current.as_deref() != Some(front.workspace.as_str())
            && !self.hand_front_on(&front, (endpoint, generation, &boot), cx)
        {
            return;
        }
        self.front_watch.gone_at = None;
        let Some(workspace) = current else {
            self.front_watch.front = None;
            self.front_watch.key = None;
            return;
        };
        let sessions = self.sessions(endpoint);
        let key = Key {
            endpoint,
            workspace: workspace.clone(),
            snapshot: self
                .live
                .snapshot
                .as_ref()
                .map(|snapshot| std::sync::Arc::as_ptr(snapshot) as usize),
            core: sessions
                .core
                .as_ref()
                .map(|core| std::sync::Arc::as_ptr(core) as usize),
        };
        if self.front_watch.key.as_ref() == Some(&key) {
            return;
        }
        let successors = successors(&self.sidebar_workspaces(cx), endpoint, &workspace);
        self.front_watch.key = Some(key);
        self.front_watch.front = Some(Front {
            endpoint,
            generation,
            boot,
            workspace,
            successors,
        });
    }

    /// The front moved from `front`: when `front` is gone from the same
    /// connection, bring its successor forward. False while an open menu
    /// keeps it waiting, so the next tick tries again. A removal's own
    /// progress dialog waits as long as it is open, as the rows above may be
    /// removed too.
    fn hand_front_on(
        &mut self,
        front: &Front,
        (endpoint, generation, boot): (usize, u64, &Option<String>),
        cx: &mut Context<Self>,
    ) -> bool {
        let same =
            front.endpoint == endpoint && front.generation == generation && &front.boot == boot;
        let sessions = self.sessions(endpoint);
        // An empty session is one not read yet, not one emptied.
        let gone = same && !sessions.is_empty() && sessions.workspace(&front.workspace).is_none();
        if !gone {
            return true;
        }
        if matches!(
            self.menu.page,
            Some(Page::RemoveMerged | Page::Dialog(WorkspaceAction::DeleteWorktree))
        ) {
            self.front_watch.gone_at = None;
            return false;
        }
        let Some(next) = front
            .successors
            .iter()
            .map(String::as_str)
            .find(|next| sessions.workspace(next).is_some())
        else {
            return true;
        };
        if self.browser_key().is_some_and(|(_, id)| id == next) {
            return true;
        }
        let id = self.endpoints[endpoint].id.clone();
        if self.navigate_endpoint(&id, NavigationTarget::Workspace(next), cx) {
            return true;
        }
        let now = Instant::now();
        let since = *self.front_watch.gone_at.get_or_insert(now);
        now.duration_since(since) >= PATIENCE
    }
}

#[cfg(test)]
mod tests;
