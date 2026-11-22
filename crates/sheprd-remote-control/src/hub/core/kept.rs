//! What Sheprd keeps for this core: its workspaces, in the user's order,
//! and the tabs of their terminals. Once any client has written the core's
//! shared document (`sheprd_session::spaces`, version above 0), it holds
//! them for every client; until then, Sheprd's saved tabs on this computer
//! do, for this computer's scope.

use super::saved::SAVED_TAB;
use sheprd_backend::TerminalId;
use sheprd_session::{
    SavedTabs,
    panes::Panes,
    spaces::{Spaces, Tab},
};
use std::collections::{BTreeMap, HashSet};

/// Where the kept workspaces and tabs are read from.
#[derive(Clone, Copy)]
pub(super) enum Kept<'a> {
    /// The core's shared document, with its tabs as the poller remapped
    /// them for recovered chats.
    Doc { spaces: &'a Spaces, tabs: &'a [Tab] },
    /// Sheprd's saved tabs of `scope`, while the document was never written.
    File {
        saved: &'a SavedTabs,
        scope: &'a str,
    },
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
    pub(super) workspace_id: &'a str,
    pub(super) zoomed: bool,
}

impl<'a> Kept<'a> {
    /// The saved tabs of `scope`, when there are both.
    pub(super) fn file(saved: Option<&'a SavedTabs>, scope: Option<&'a str>) -> Option<Self> {
        saved
            .zip(scope)
            .map(|(saved, scope)| Self::File { saved, scope })
    }

    pub(super) fn workspaces(self) -> Vec<KeptWorkspace<'a>> {
        match self {
            Self::Doc { spaces, .. } => spaces
                .workspaces()
                .iter()
                .map(|workspace| KeptWorkspace {
                    cwd: &workspace.cwd,
                    id: &workspace.id,
                    label: &workspace.label,
                })
                .collect(),
            Self::File { saved, scope } => saved
                .workspaces(scope)
                .map(|workspace| KeptWorkspace {
                    cwd: &workspace.cwd,
                    id: &workspace.id,
                    label: &workspace.label,
                })
                .collect(),
        }
    }

    /// Whether workspace `id` is kept.
    pub(super) fn keeps(self, id: &str) -> bool {
        self.workspaces().iter().any(|workspace| workspace.id == id)
    }

    /// Every tab, in the order kept. Mirrors are left out: the terminal
    /// they watch is listed where it types.
    pub(super) fn tabs(self) -> Vec<KeptTab<'a>> {
        match self {
            Self::Doc { tabs, .. } => tabs
                .iter()
                .map(|tab| KeptTab {
                    active: None,
                    name: tab.name.as_deref().filter(|name| !name.trim().is_empty()),
                    names: Some(&tab.names),
                    panes: &tab.panes,
                    target: format!("{SAVED_TAB}{}", tab.id),
                    title: &tab.title,
                    workspace_id: &tab.workspace_id,
                    zoomed: tab.zoomed.is_some(),
                })
                .collect(),
            Self::File { saved, scope } => saved
                .core_tabs(scope)
                .map(|tab| KeptTab {
                    active: saved.tab_active(scope, tab.id),
                    name: saved.tab_name(scope, tab.id),
                    names: saved.pane_names(scope, tab.id),
                    panes: tab.panes,
                    target: format!("{SAVED_TAB}{}", tab.id),
                    title: tab.title,
                    workspace_id: tab.workspace_id,
                    zoomed: saved.tab_zoomed(scope, tab.id).is_some(),
                })
                .collect(),
        }
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
