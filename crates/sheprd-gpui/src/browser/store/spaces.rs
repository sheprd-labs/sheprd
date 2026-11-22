//! The store's side of a core's shared document (`sheprd_session::spaces`):
//! what one scope's kept workspaces and herdr-core tabs look like as the
//! document holds them, and taking a document's in. What only one window
//! shows stays in the store alone: which pane is in use, mirror tabs, and
//! pages.

use super::{Location, Place, Scope, Store, Tab};
use crate::core_terminals::panes::Panes;
use serde::{Deserialize, Serialize};
use sheprd_backend::TerminalId;
use sheprd_session::spaces::{self, Adopted};
use std::collections::{BTreeSet, HashSet};

mod mirror;

pub(crate) use mirror::Pending;

/// One scope's kept workspaces and shared tabs, as a core's document holds
/// them, in the user's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct View {
    pub(crate) tabs: Vec<spaces::Tab>,
    pub(crate) workspaces: Vec<spaces::Workspace>,
}

impl View {
    pub(crate) fn of(spaces: &spaces::Spaces) -> Self {
        Self {
            tabs: spaces.tabs().to_vec(),
            workspaces: spaces.workspaces().to_vec(),
        }
    }

    pub(crate) fn workspace(&self, id: &str) -> Option<&spaces::Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub(crate) fn tab(&self, id: &str) -> Option<&spaces::Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    /// The ids it holds at `version`, as the store keeps them across
    /// restarts.
    pub(crate) fn synced(&self, version: u64) -> Synced {
        Synced {
            version,
            tabs: self.tabs.iter().map(|tab| tab.id.clone()).collect(),
            workspaces: (self.workspaces.iter())
                .map(|workspace| workspace.id.clone())
                .collect(),
        }
    }

    /// Whether it holds `record`.
    pub(crate) fn has(&self, record: Record<'_>) -> bool {
        match record {
            Record::Tab(id) => self.tab(id).is_some(),
            Record::Workspace(id) => self.workspace(id).is_some(),
        }
    }

    /// The records `keep` keeps, in order.
    pub(crate) fn only(mut self, keep: impl Fn(Record<'_>) -> bool) -> Self {
        self.workspaces
            .retain(|workspace| keep(Record::Workspace(&workspace.id)));
        self.tabs.retain(|tab| keep(Record::Tab(&tab.id)));
        self
    }

    /// The view under the ids [`Spaces::adopt`](spaces::Spaces::adopt)
    /// gave its records; one it gave none keeps its own.
    pub(crate) fn renamed(mut self, adopted: &Adopted) -> Self {
        let rename = |map: &std::collections::HashMap<String, String>, id: &mut String| {
            if let Some(new) = map.get(id.as_str()) {
                id.clone_from(new);
            }
        };
        for workspace in &mut self.workspaces {
            rename(&adopted.workspaces, &mut workspace.id);
        }
        let mut seen = HashSet::new();
        self.workspaces
            .retain(|workspace| seen.insert(workspace.id.clone()));
        for tab in &mut self.tabs {
            rename(&adopted.workspaces, &mut tab.workspace_id);
            rename(&adopted.tabs, &mut tab.id);
        }
        self
    }
}

/// The ids a core's document held when last read, kept with the tabs so a
/// record that went from the document while the app was closed is told
/// from one the app made and has not written yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Synced {
    #[serde(default)]
    pub(crate) tabs: BTreeSet<String>,
    /// The document's version then: one read lower was written anew, as
    /// when the core's state was wiped, and these ids say nothing of it.
    #[serde(default)]
    pub(crate) version: u64,
    #[serde(default)]
    pub(crate) workspaces: BTreeSet<String>,
}

impl Store {
    /// `scope`'s kept workspaces and the herdr-core tabs that type into
    /// their terminals, as the document holds them. A tab with no document
    /// id yet is left out ([`Self::name_shared_tabs`]).
    pub(crate) fn shared_view(&self, scope: &Scope) -> View {
        let workspaces: Vec<spaces::Workspace> = self
            .kept_workspaces(scope)
            .map(|workspace| spaces::Workspace {
                cwd: workspace.cwd.clone(),
                custom: workspace.custom,
                id: workspace.id.clone(),
                label: workspace.label.clone(),
            })
            .collect();
        let tabs = self
            .in_scope(scope)
            .filter(|tab| workspaces.iter().any(|kept| kept.id == tab.workspace_id))
            .filter_map(shared_tab)
            .collect();
        View { tabs, workspaces }
    }

    /// Whether a herdr-core tab of `scope`'s kept workspaces has no
    /// document id yet.
    pub(crate) fn unnamed_shared_tabs(&self, scope: &Scope) -> bool {
        self.in_scope(scope).any(|tab| {
            tab.space.is_none()
                && drives(tab)
                && self.kept_workspace(scope, &tab.workspace_id).is_some()
        })
    }

    /// Gives each herdr-core tab of `scope`'s kept workspaces that types
    /// into its terminals a document id of its own, from `new_id`. Whether
    /// any took one.
    pub(crate) fn name_shared_tabs(
        &mut self,
        scope: &Scope,
        mut new_id: impl FnMut() -> String,
    ) -> bool {
        let kept: HashSet<String> = self
            .kept_workspaces(scope)
            .map(|workspace| workspace.id.clone())
            .collect();
        let mut named = false;
        for tab in &mut self.tabs {
            if tab.space.is_none()
                && &tab.scope == scope
                && kept.contains(&tab.workspace_id)
                && drives(tab)
            {
                tab.space = Some(new_id());
                named = true;
            }
        }
        if named {
            self.save();
        }
        named
    }

    /// What `scope`'s document held when last read; empty when the app
    /// never read it.
    pub(crate) fn synced(&self, scope: &Scope) -> Option<&Synced> {
        self.synced.get(scope.as_str())
    }

    pub(crate) fn set_synced(&mut self, scope: &Scope, synced: Option<Synced>) {
        let key = scope.as_str();
        if self.synced.get(key) == synced.as_ref() {
            return;
        }
        match synced {
            Some(synced) => self.synced.insert(key.to_owned(), synced),
            None => self.synced.remove(key),
        };
        self.save();
    }

    /// Takes the ids [`Spaces::adopt`](spaces::Spaces::adopt) gave what the
    /// app kept for `scope`: a workspace and the tabs in it under its id in
    /// the document, and each tab under its tab id.
    pub(crate) fn take_adopted(&mut self, scope: &Scope, adopted: &Adopted) {
        let mut changed = false;
        for workspace in &mut self.workspaces {
            if workspace.scope != scope.as_str() {
                continue;
            }
            if let Some(id) = adopted.workspaces.get(&workspace.id)
                && *id != workspace.id
            {
                workspace.id.clone_from(id);
                changed = true;
            }
        }
        // A workspace that joined one kept already appears twice now.
        let mut seen = HashSet::new();
        self.workspaces.retain(|workspace| {
            workspace.scope != scope.as_str() || seen.insert(workspace.id.clone())
        });
        for tab in &mut self.tabs {
            if &tab.scope != scope {
                continue;
            }
            if let Some(id) = adopted.workspaces.get(&tab.workspace_id)
                && *id != tab.workspace_id
            {
                tab.workspace_id.clone_from(id);
                changed = true;
            }
            if let Some(id) = (tab.space.as_ref()).and_then(|space| adopted.tabs.get(space))
                && Some(id) != tab.space.as_ref()
            {
                tab.space = Some(id.clone());
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }
}

/// A record of a shared document, by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Record<'a> {
    Tab(&'a str),
    Workspace(&'a str),
}

/// Whether `tab` types into herdr-core terminals: a shared tab, unlike a
/// page or a mirror that only watches.
fn drives(tab: &Tab) -> bool {
    tab.place == Place::Group
        && matches!(
            tab.location,
            Some(Location::CoreTerminal { observe: false, .. })
        )
}

/// `tab` as the document holds it, once it has a document id.
fn shared_tab(tab: &Tab) -> Option<spaces::Tab> {
    if !drives(tab) {
        return None;
    }
    let Some(Location::CoreTerminal {
        panes,
        zoomed,
        name,
        names,
        ..
    }) = &tab.location
    else {
        return None;
    };
    Some(spaces::Tab {
        id: tab.space.clone()?,
        workspace_id: tab.workspace_id.clone(),
        title: tab.title.clone(),
        name: name.clone(),
        names: names
            .iter()
            .filter(|(terminal, _)| panes.contains(terminal))
            .map(|(terminal, name)| (terminal.as_str().to_owned(), name.clone()))
            .collect(),
        panes: panes.clone(),
        zoomed: zoomed.clone().filter(|zoomed| panes.contains(zoomed)),
    })
}

/// The location a shared tab saves with, keeping the pane this app had in
/// use when the document still holds it.
fn location_of(shared: &spaces::Tab, active: Option<TerminalId>) -> Location {
    let panes: Panes = shared.panes.clone();
    Location::CoreTerminal {
        active: active.filter(|active| panes.contains(active)),
        zoomed: shared
            .zoomed
            .clone()
            .filter(|zoomed| panes.contains(zoomed)),
        name: shared.name.clone(),
        names: shared
            .names
            .iter()
            .filter_map(|(terminal, name)| {
                let terminal = TerminalId::new(terminal.clone()).ok()?;
                panes.contains(&terminal).then(|| (terminal, name.clone()))
            })
            .collect(),
        observe: false,
        panes,
    }
}

/// How the document's tab ids start.
pub(crate) const TAB_PREFIX: &str = "tab-";

/// A new document id: `prefix` and 16 random hex digits, so ids two
/// clients hand out at once never meet, as the `space-N` counter did.
pub(crate) fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; 8];
    let random = match getrandom::fill(&mut bytes) {
        Ok(()) => u64::from_le_bytes(bytes),
        // No system randomness: a hasher seeded per process, over the time
        // and a count, still keeps ids apart.
        Err(_) => {
            use std::hash::{BuildHasher, Hasher};
            static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u64(COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            hasher.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |time| time.as_nanos()),
            );
            hasher.finish()
        }
    };
    format!("{prefix}{random:016x}")
}
