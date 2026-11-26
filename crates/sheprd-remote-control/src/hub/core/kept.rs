//! What Sheprd keeps for this core: its workspaces, in the user's order,
//! and the tabs of their terminals. Once any client has written the core's
//! shared document (`sheprd_session::spaces`, version above 0), it holds
//! them for every client; until then, Sheprd's saved tabs on this computer
//! do, for this computer's scope.

use super::saved::{LOCAL_TAB, SAVED_TAB};
use sheprd_backend::TerminalId;
use sheprd_session::{
    SavedTabs,
    panes::Panes,
    spaces::{Spaces, Tab},
};
use std::collections::{BTreeMap, HashSet};

/// Where the kept workspaces and tabs are read from.
#[derive(Clone, Copy)]
pub(super) struct Kept<'a> {
    pub(super) doc: Option<(&'a Spaces, &'a [Tab])>,
    pub(super) file: Option<&'a SavedTabs>,
    pub(super) primary: Option<&'a str>,
    pub(super) named: &'a BTreeMap<String, String>,
}

/// A kept workspace.
pub(super) struct KeptWorkspace<'a> {
    pub(super) cwd: &'a str,
    pub(super) id: &'a str,
    pub(super) label: &'a str,
}

/// A kept tab of the core's terminals.
pub(super) struct KeptTab<'a> {
    /// The pane the user last selected in it; the document keeps none, as
    /// that is each window's own.
    pub(super) active: Option<&'a TerminalId>,
    /// The name the user gave it, shown over its title.
    pub(super) name: Option<&'a str>,
    /// Names the user gave its panes, by terminal address.
    pub(super) names: Option<&'a BTreeMap<String, String>>,
    pub(super) panes: &'a Panes,
    /// Its id in the layout: [`SAVED_TAB`] and the id its keeper gave it.
    pub(super) target: String,
    pub(super) title: &'a str,
    pub(super) workspace_id: std::borrow::Cow<'a, str>,
    pub(super) zoomed: bool,
}

impl<'a> Kept<'a> {
    pub(super) fn workspaces(self) -> Vec<KeptWorkspace<'a>> {
        if let Some((spaces, _)) = self.doc {
            return spaces
                .workspaces()
                .iter()
                .map(|w| KeptWorkspace {
                    cwd: &w.cwd,
                    id: &w.id,
                    label: &w.label,
                })
                .collect();
        }
        self.file
            .zip(self.primary)
            .into_iter()
            .flat_map(|(saved, scope)| saved.workspaces(scope))
            .map(|w| KeptWorkspace {
                cwd: &w.cwd,
                id: &w.id,
                label: &w.label,
            })
            .collect()
    }

    /// Whether workspace `id` is kept.
    pub(super) fn keeps(self, id: &str) -> bool {
        self.workspaces().iter().any(|workspace| workspace.id == id)
    }

    /// Every tab, in the order kept. Mirrors are left out: the terminal
    /// they watch is listed where it types.
    pub(super) fn tabs(self) -> Vec<KeptTab<'a>> {
        let shared = self.doc.map_or(&[][..], |(_, tabs)| tabs);
        let mut kept: Vec<_> = shared
            .iter()
            .map(|tab| KeptTab {
                active: None,
                name: tab.name.as_deref().filter(|name| !name.trim().is_empty()),
                names: Some(&tab.names),
                panes: &tab.panes,
                target: format!("{SAVED_TAB}{}", tab.id),
                title: &tab.title,
                workspace_id: std::borrow::Cow::Borrowed(&tab.workspace_id),
                zoomed: tab.zoomed.is_some(),
            })
            .collect();

        let Some(saved) = self.file else {
            return kept;
        };
        let mut held: HashSet<_> = shared
            .iter()
            .flat_map(|tab| tab.panes.terminals())
            .collect();
        for (name, scope) in self.primary.into_iter().map(|scope| (None, scope)).chain(
            self.named
                .iter()
                .map(|(name, scope)| (Some(name.as_str()), scope.as_str())),
        ) {
            let legacy = self.doc.is_none() && name.is_none();
            let prefix = if legacy { SAVED_TAB } else { LOCAL_TAB };
            let kept_ids: HashSet<_> = saved.workspaces(scope).map(|w| w.id.as_str()).collect();
            for mut tab in file_tabs(saved, scope, prefix, name) {
                let id = tab.workspace_id.as_ref();
                if !legacy
                    && (id == sheprd_session::DEFAULT_WORKSPACE
                        || id.starts_with(sheprd_session::spaces::ID_PREFIX)
                        || kept_ids.contains(id)
                        || self
                            .doc
                            .is_some_and(|(spaces, _)| spaces.workspace(id).is_some()))
                {
                    continue;
                }
                let panes = tab.panes.terminals();
                if panes.iter().any(|pane| held.contains(pane)) {
                    continue;
                }
                held.extend(panes);
                if name.is_some() {
                    tab.workspace_id = crate::hub::sessions::join_id(name, id).into();
                }
                kept.push(tab);
            }
        }
        kept
    }

    /// Every terminal a kept tab holds.
    pub(super) fn held(self) -> HashSet<String> {
        self.tabs()
            .iter()
            .flat_map(|tab| tab.panes.terminals())
            .map(|terminal| terminal.as_str().to_owned())
            .collect()
    }
}

/// Placement of managed tmux panes in the same saved tabs the core reads.
/// A named Herdr session can have its own w1 on this computer.
#[cfg(feature = "tmux")]
pub(in crate::hub) fn tmux_placements(
    file: Option<&SavedTabs>,
    primary: Option<&str>,
    named: &BTreeMap<String, String>,
) -> std::collections::HashMap<TerminalId, (String, Option<String>)> {
    let kept = Kept {
        file,
        primary,
        named,
        doc: None,
    };
    let mut locations = std::collections::HashMap::new();
    for tab in kept.tabs() {
        for pane in tab.panes.terminals().into_iter().filter(|id| id.is_tmux()) {
            let name = tab
                .names
                .and_then(|names| names.get(pane.as_str()))
                .cloned();
            locations
                .entry(pane.clone())
                .or_insert_with(|| (tab.workspace_id.to_string(), name));
        }
    }
    locations
}

fn file_tabs<'a>(
    saved: &'a SavedTabs,
    scope: &'a str,
    prefix: &'static str,
    session: Option<&'a str>,
) -> impl Iterator<Item = KeptTab<'a>> {
    saved.core_tabs(scope).map(move |tab| KeptTab {
        active: saved.tab_active(scope, tab.id),
        name: saved.tab_name(scope, tab.id),
        names: saved.pane_names(scope, tab.id),
        panes: tab.panes,
        target: format!(
            "{prefix}{}",
            crate::hub::sessions::join_id(session, &tab.id.to_string())
        ),
        title: tab.title,
        workspace_id: tab.workspace_id.into(),
        zoomed: saved.tab_zoomed(scope, tab.id).is_some(),
    })
}

/// `tab` with each recovered chat's terminal replaced by the one that took
/// its place, keeping its pane, name, and zoom.
pub(super) fn remap(tab: &mut Tab, replacements: &BTreeMap<TerminalId, TerminalId>) {
    for (old, new) in replacements {
        if !tab.panes.replace(old, new.clone()) {
            continue;
        }
        if let Some(name) = tab.names.remove(old.as_str()) {
            tab.names.insert(new.as_str().to_owned(), name);
        }
        if tab.zoomed.as_ref() == Some(old) {
            tab.zoomed = Some(new.clone());
        }
    }
}
