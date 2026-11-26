//! Taking a core's document into the store, merged three ways: against
//! what the document held when the app last read it, a record changed only
//! there takes the document's, one changed here and not written yet keeps
//! the app's, and one the document dropped goes, ending nothing here.

use super::{Record, View, location_of, shared_tab};
use crate::browser::store::{MAX_TABS, Place, Scope, Store, Tab, TabId, Zoom, clean_title, kept};
use crate::core_terminals::panes::{Axis, Panes};
use sheprd_backend::TerminalId;
use sheprd_session::{
    SavedWorkspace,
    spaces::{self, Edit, TabEdit},
};
use std::collections::{BTreeSet, HashMap, HashSet};

/// How many dropped tabs are remembered: a window forgets a tab within a
/// tick of its going, and tab ids are never reused.
const MAX_DROPPED: usize = 1024;

/// What the app changed and has not written yet, by record: the edits
/// from the document as last read to the store now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pending {
    /// Tabs closed here.
    pub(crate) closed: HashSet<String>,
    /// Workspaces moved to a folder here: the folder stays this app's,
    /// and the name and whether the user gave it are the document's.
    pub(crate) followed: HashSet<String>,
    /// Workspaces forgotten here.
    pub(crate) forgotten: HashSet<String>,
    /// Whether workspaces moved here.
    pub(crate) moved: bool,
    /// Tabs opened here.
    pub(crate) opened: HashSet<String>,
    /// What changed here of each tab the document holds.
    pub(crate) tabs: HashMap<String, TabFields>,
    /// Workspaces kept or renamed here.
    pub(crate) workspaces: HashSet<String>,
}

/// The fields of one tab changed here and not written yet; the others take
/// what the document holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabFields {
    pub(crate) moved: bool,
    pub(crate) name: bool,
    /// Panes whose names changed, by terminal address.
    pub(crate) names: BTreeSet<String>,
    /// Whether panes were resized or swapped here.
    pub(crate) layout: bool,
    /// Panes closed here.
    pub(crate) removed: BTreeSet<TerminalId>,
    /// Panes opened here: each `new` beside `at`.
    pub(crate) splits: Vec<(TerminalId, Axis, TerminalId)>,
    pub(crate) zoom: bool,
}

impl TabFields {
    /// The panes changed here made again to `theirs`, the document's: the
    /// panes closed and opened, and the layout when the tab then holds the
    /// same terminals as `mine`, so another client's split or closed pane
    /// stays.
    fn panes(&self, theirs: &Panes, mine: &Panes) -> Panes {
        let mut panes = theirs.clone();
        // Closed first, so a pane opened here has room in a full tab; a
        // last pane closes once its replacement is there.
        let mut last = Vec::new();
        for terminal in &self.removed {
            if panes.count() > 1 {
                panes.remove(terminal);
            } else {
                last.push(terminal);
            }
        }
        for (at, axis, new) in &self.splits {
            if panes.contains(new) {
                continue;
            }
            // Beside the pane it was opened beside, or, when that one closed
            // since (here or elsewhere), beside one still there: the pane
            // itself is what must stay.
            let at = if panes.contains(at) {
                at.clone()
            } else {
                panes.first().clone()
            };
            // A split the layout refuses is left to the document.
            let _ = panes.split(&at, *axis, new.clone());
        }
        for terminal in last {
            if panes.count() > 1 {
                panes.remove(terminal);
            }
        }
        let mut held = panes.terminals();
        let mut laid_out = mine.terminals();
        held.sort_unstable();
        laid_out.sort_unstable();
        if self.layout && held == laid_out {
            mine.clone()
        } else {
            panes
        }
    }

    fn take(&mut self, edit: &TabEdit) {
        match edit {
            TabEdit::Move { .. } => self.moved = true,
            TabEdit::Rename { .. } => self.name = true,
            TabEdit::RenamePane { terminal, .. } => {
                self.names.insert(terminal.as_str().to_owned());
            }
            TabEdit::RemovePane { terminal } => {
                self.removed.insert(terminal.clone());
            }
            TabEdit::SetPanes { .. } => self.layout = true,
            TabEdit::Split { at, axis, new } => self.splits.push((at.clone(), *axis, new.clone())),
            // Titles are not written.
            TabEdit::Retitle { .. } => {}
            TabEdit::Zoom { .. } => self.zoom = true,
        }
    }

    /// `theirs`, the document's tab, with the fields changed here taken
    /// from `mine`.
    fn merged(&self, theirs: &spaces::Tab, mine: &spaces::Tab) -> spaces::Tab {
        let mut tab = theirs.clone();
        if self.moved {
            tab.workspace_id.clone_from(&mine.workspace_id);
        }
        tab.panes = self.panes(&theirs.panes, &mine.panes);
        if self.name {
            tab.name.clone_from(&mine.name);
        }
        for terminal in &self.names {
            match mine.names.get(terminal) {
                Some(name) => tab.names.insert(terminal.clone(), name.clone()),
                None => tab.names.remove(terminal),
            };
        }
        if self.zoom {
            tab.zoomed.clone_from(&mine.zoomed);
        }
        tab
    }
}

impl Pending {
    pub(crate) fn of(edits: &[Edit]) -> Self {
        let mut pending = Self::default();
        for edit in edits {
            match edit {
                Edit::CloseTab { id } => {
                    pending.closed.insert(id.clone());
                }
                Edit::ForgetWorkspace { id } => {
                    pending.forgotten.insert(id.clone());
                }
                Edit::FollowWorkspace { id, .. } => {
                    pending.followed.insert(id.clone());
                }
                Edit::KeepWorkspace(workspace) => {
                    pending.workspaces.insert(workspace.id.clone());
                }
                Edit::MoveWorkspace { .. } => pending.moved = true,
                Edit::OpenTab(tab) => {
                    pending.opened.insert(tab.id.clone());
                }
                Edit::RenameWorkspace { id, .. } => {
                    pending.workspaces.insert(id.clone());
                }
                Edit::Tab { id, edit } => pending.tabs.entry(id.clone()).or_default().take(edit),
            }
        }
        pending
    }

    /// Whether `record` was removed here.
    pub(crate) fn removed(&self, record: Record<'_>) -> bool {
        match record {
            Record::Tab(id) => self.closed.contains(id),
            Record::Workspace(id) => self.forgotten.contains(id),
        }
    }
}

impl Store {
    /// Takes `document` in as `scope`'s kept workspaces and shared tabs,
    /// keeping what one window shows (the pane in use, a tab's title) and
    /// what `pending` says changed here. A record the document does not
    /// hold goes only when `gone` says the document held it before; a tab
    /// that goes is [dropped](Self::dropped_shared), so no window ends its
    /// terminals. Whether anything changed.
    pub(crate) fn mirror_shared(
        &mut self,
        scope: &Scope,
        document: &View,
        pending: &Pending,
        gone: impl Fn(Record<'_>) -> bool,
    ) -> bool {
        let before = (self.workspaces.clone(), self.tabs.clone());
        self.mirror_workspaces(scope, document, pending, &gone);
        self.mirror_tabs(scope, document, pending, &gone);
        let changed = before != (self.workspaces.clone(), self.tabs.clone());
        if changed {
            self.save();
        }
        changed
    }

    /// Whether `tab` went because its core's document dropped it.
    pub(crate) fn dropped_shared(&self, tab: TabId) -> bool {
        self.dropped.contains(&tab)
    }

    fn drop_tabs(&mut self, mut goes: impl FnMut(&Tab) -> bool) {
        let mut dropped = Vec::new();
        self.tabs.retain(|tab| {
            let keep = !goes(tab);
            if !keep {
                dropped.push(tab.id);
            }
            keep
        });
        for id in dropped {
            if self.dropped.len() >= MAX_DROPPED {
                self.dropped.pop_front();
            }
            self.dropped.push_back(id);
        }
    }

    fn mirror_workspaces(
        &mut self,
        scope: &Scope,
        document: &View,
        pending: &Pending,
        gone: &impl Fn(Record<'_>) -> bool,
    ) {
        let ours = |workspace: &SavedWorkspace| workspace.scope == scope.as_str();
        let at = self.workspaces.iter().position(ours);
        let mine: Vec<SavedWorkspace> = self
            .workspaces
            .iter()
            .filter(|w| ours(w))
            .cloned()
            .collect();
        let held = |id: &str| mine.iter().find(|workspace| workspace.id == id);
        let others = self.workspaces.len() - mine.len();
        // Those already here stay; a new one comes while there is room.
        let staying = |workspace: &SavedWorkspace| match document.workspace(&workspace.id) {
            Some(_) => !pending.forgotten.contains(&workspace.id),
            None => !gone(Record::Workspace(&workspace.id)),
        };
        let mut room = kept::MAX_WORKSPACES
            .saturating_sub(others)
            .saturating_sub(mine.iter().filter(|w| staying(w)).count());
        let mut taken: Vec<SavedWorkspace> = Vec::new();
        for workspace in &document.workspaces {
            if pending.forgotten.contains(&workspace.id) {
                continue;
            }
            let theirs = || SavedWorkspace {
                cwd: workspace.cwd.clone(),
                custom: workspace.custom,
                id: workspace.id.clone(),
                label: workspace.label.clone(),
                scope: scope.as_str().to_owned(),
            };
            let record = match held(&workspace.id) {
                // Kept or renamed here, not written yet: the app's stands.
                Some(here) if pending.workspaces.contains(&workspace.id) => here.clone(),
                // Moved here, not written yet: only the folder stands, so a
                // rename from elsewhere is not undone.
                Some(here) if pending.followed.contains(&workspace.id) => SavedWorkspace {
                    cwd: here.cwd.clone(),
                    ..theirs()
                },
                Some(_) => theirs(),
                None if room == 0 => continue,
                None => {
                    room -= 1;
                    theirs()
                }
            };
            taken.push(record);
        }
        // Workspaces made here and not written yet stay, after the
        // document's.
        taken.extend(
            (mine.iter())
                .filter(|workspace| {
                    document.workspace(&workspace.id).is_none() && staying(workspace)
                })
                .cloned(),
        );
        if pending.moved {
            // Moved here and not written yet: the order stays this app's.
            let place = |id: &str| mine.iter().position(|workspace| workspace.id == id);
            taken.sort_by_key(|workspace| place(&workspace.id).unwrap_or(usize::MAX));
        }
        let gone_ids: HashSet<String> = (mine.iter())
            .filter(|workspace| !taken.iter().any(|kept| kept.id == workspace.id))
            .map(|workspace| workspace.id.clone())
            .collect();
        self.drop_tabs(|tab| &tab.scope == scope && gone_ids.contains(&tab.workspace_id));
        self.workspaces.retain(|workspace| !ours(workspace));
        let at = at
            .unwrap_or(self.workspaces.len())
            .min(self.workspaces.len());
        self.workspaces.splice(at..at, taken);
    }

    fn mirror_tabs(
        &mut self,
        scope: &Scope,
        document: &View,
        pending: &Pending,
        gone: &impl Fn(Record<'_>) -> bool,
    ) {
        self.drop_tabs(|tab| {
            tab.space.as_deref().is_some_and(|space| {
                &tab.scope == scope && document.tab(space).is_none() && gone(Record::Tab(space))
            })
        });
        for shared in &document.tabs {
            if pending.closed.contains(&shared.id)
                || self.kept_workspace(scope, &shared.workspace_id).is_none()
            {
                continue;
            }
            let room = self.tabs.len() < MAX_TABS;
            let held = self
                .tabs
                .iter_mut()
                .find(|tab| &tab.scope == scope && tab.space.as_deref() == Some(&shared.id));
            match held {
                Some(_) if pending.opened.contains(&shared.id) => {}
                Some(tab) => {
                    let active = match &tab.location {
                        Some(crate::browser::Location::CoreTerminal { active, .. }) => {
                            active.clone()
                        }
                        _ => None,
                    };
                    // Changed here on some fields: those stay, the rest
                    // take the document's.
                    let merged = (pending.tabs.get(&shared.id))
                        .zip(shared_tab(tab))
                        .map(|(fields, mine)| fields.merged(shared, &mine));
                    let shared = merged.as_ref().unwrap_or(shared);
                    tab.location = Some(location_of(shared, active));
                    tab.workspace_id.clone_from(&shared.workspace_id);
                    if tab.title.is_empty() {
                        tab.title = clean_title(&shared.title);
                    }
                }
                None if room => {
                    let location = location_of(shared, None);
                    // Bounded as the store keeps titles: a longer one would
                    // make the saved tabs unreadable.
                    let title = Some(clean_title(&shared.title))
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| location.default_title());
                    let id = TabId(self.next);
                    self.next += 1;
                    self.tabs.push(Tab {
                        id,
                        scope: scope.clone(),
                        workspace_id: shared.workspace_id.clone(),
                        location: Some(location),
                        title,
                        origin: None,
                        place: Place::Group,
                        space: Some(shared.id.clone()),
                        zoom: Zoom::default(),
                    });
                }
                None => tracing::warn!("No room for a shared herdr-core tab"),
            }
        }
    }
}
