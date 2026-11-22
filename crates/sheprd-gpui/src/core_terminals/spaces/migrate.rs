//! The first read of a core's document this run: what the app kept for
//! that core and the document does not hold yet joins it, through
//! [`Spaces::adopt`]. Records the document held when the app last read it
//! and holds no more went while the app was away, and are not offered.

use crate::browser::shared::{Synced, TAB_PREFIX, View, new_id};
use sheprd_session::spaces::{Adopted, Edit, Error, ID_PREFIX, Spaces, Tab};

/// What the app kept for one core, offered to its document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Offer {
    /// The ids the document held when the app last read it.
    pub(crate) synced: Synced,
    pub(crate) view: View,
}

impl Offer {
    /// The offer for a document read at `version`, and whether it was
    /// written anew since the app last read it: one of a lower version, as
    /// when the core's state was wiped, never held what the app's ids say,
    /// so everything the app keeps is offered to it.
    pub(crate) fn at(&self, version: u64) -> (Self, bool) {
        let reset = version < self.synced.version;
        let mut offer = self.clone();
        if reset {
            offer.synced = Synced::default();
        }
        (offer, reset)
    }
}

/// Adds `offer` to `spaces`, and answers the document ids each offered
/// workspace and tab took. Runs again when another client wrote first.
pub(crate) fn migrate(spaces: &mut Spaces, offer: &Offer) -> Result<Adopted, Error> {
    let synced = &offer.synced;
    let mut same = Adopted::default();
    let mut workspaces = Vec::new();
    // An offered id the document gives another workspace takes a new one
    // first, so `adopt` hands out no id of the wrong kind.
    let mut renamed = std::collections::HashMap::new();
    for workspace in &offer.view.workspaces {
        let held = spaces.workspace(&workspace.id).is_some();
        // The workspace a window without Herdr 1 gathers its tabs in is
        // the same one on every client.
        let shared = synced.workspaces.contains(&workspace.id)
            || workspace.id == sheprd_session::DEFAULT_WORKSPACE;
        if held && shared {
            same.workspaces
                .insert(workspace.id.clone(), workspace.id.clone());
            continue;
        }
        if synced.workspaces.contains(&workspace.id) {
            // Forgotten by another client while this app was away.
            continue;
        }
        let id = if held {
            new_id(ID_PREFIX)
        } else {
            workspace.id.clone()
        };
        renamed.insert(id.clone(), workspace.id.clone());
        workspaces.push(sheprd_session::spaces::Workspace {
            id,
            ..workspace.clone()
        });
    }
    let mut fresh = Vec::new();
    let mut joining = Vec::new();
    for tab in &offer.view.tabs {
        if synced.tabs.contains(&tab.id) {
            if spaces.tab(&tab.id).is_some() {
                same.tabs.insert(tab.id.clone(), tab.id.clone());
            }
            continue;
        }
        if same.workspaces.contains_key(&tab.workspace_id) {
            joining.push(tab.clone());
            continue;
        }
        let Some(workspace_id) = renamed
            .iter()
            .find(|(_, was)| **was == tab.workspace_id)
            .map(|(id, _)| id.clone())
        else {
            continue;
        };
        let id = if spaces.tab(&tab.id).is_some() {
            new_id(TAB_PREFIX)
        } else {
            tab.id.clone()
        };
        fresh.push((
            tab.id.clone(),
            Tab {
                id,
                workspace_id,
                ..tab.clone()
            },
        ));
    }
    let offered: Vec<Tab> = fresh.iter().map(|(_, tab)| tab.clone()).collect();
    let taken = spaces.adopt(workspaces, offered, || new_id(ID_PREFIX))?;
    let mut adopted = same;
    for (id, joined) in taken.workspaces {
        let was = renamed.get(&id).cloned().unwrap_or(id);
        adopted.workspaces.insert(was, joined);
    }
    for (was, tab) in fresh {
        if let Some(id) = taken.tabs.get(&tab.id) {
            adopted.tabs.insert(was, id.clone());
        }
    }
    // Tabs opened here in a workspace the document already shares: in,
    // unless another tab there holds one of their terminals.
    for tab in joining {
        let held = (tab.panes.terminals().into_iter())
            .any(|terminal| spaces.tab_holding(terminal).is_some());
        if held {
            continue;
        }
        let was = tab.id.clone();
        let id = if spaces.tab(&tab.id).is_some() {
            new_id(TAB_PREFIX)
        } else {
            tab.id.clone()
        };
        match spaces.apply(Edit::OpenTab(Tab {
            id: id.clone(),
            ..tab
        })) {
            // Past the byte bound, it is left out as `adopt` leaves one.
            Ok(()) if matches!(spaces.to_value(), Err(Error::Large)) => {
                spaces.apply(Edit::CloseTab { id })?;
            }
            Ok(()) => {
                adopted.tabs.insert(was, id);
            }
            // Left out, as `adopt` leaves out what does not fit.
            Err(
                Error::Full
                | Error::Held(_)
                | Error::Invalid
                | Error::Large
                | Error::Missing(_)
                | Error::Taken(_),
            ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(adopted)
}
