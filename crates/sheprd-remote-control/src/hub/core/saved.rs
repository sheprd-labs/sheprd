//! Which of Sheprd's kept tabs a Herdr 2 pane is in: a tab of the core's
//! shared document or a desktop tab in a Herdr 1 workspace.
//! While Sheprd runs, only it changes such a tab's layout
//! (`control::layout`): a phone names the tab and the pane's terminal, and
//! Sheprd splits or zooms it as the desktop would; while it is closed, the
//! daemon does (`super::arrange`).

use super::{Source, lock, pane_id};
use crate::hub::Hub;

/// How a kept tab's id reads in the layout, before the id its keeper gave.
pub(super) const SAVED_TAB: &str = "core:tab:";

/// A desktop-file tab beside shared tabs. Their document IDs are opaque.
pub(super) const LOCAL_TAB: &str = "core:local-tab:";

pub(super) fn is_saved_tab(tab: &str) -> bool {
    tab.starts_with(SAVED_TAB) || tab.starts_with(LOCAL_TAB)
}

/// A tab Sheprd keeps for the core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TabRef {
    /// A tab of the core's shared document, by its id there.
    Doc(String),
    /// One of Sheprd's saved tabs, by the id Sheprd gave it; 0 for a workspace change.
    File(u64),
    Named {
        session: String,
        id: u64,
    },
}

impl Source {
    /// The kept tab `tab` names, and the address of `pane`'s terminal;
    /// `None` when either is not this core's. A numeric id names a saved
    /// tab only while the shared document stands aside.
    pub(super) fn saved_target(&self, tab: &str, pane: &str) -> Option<(TabRef, String)> {
        let address = self.terminal(pane).ok()?.1.address;
        let poller = lock(&self.poller);
        let id = if let Some(id) = tab.strip_prefix(LOCAL_TAB) {
            if !poller.local_tab_holds(self.scope.as_deref(), id, &address) {
                return None;
            }
            let (session, id) = crate::hub::sessions::split_id(id);
            let id = id.parse().ok()?;
            match session {
                Some(name) => TabRef::Named {
                    session: name.to_owned(),
                    id,
                },
                None => TabRef::File(id),
            }
        } else {
            let id = tab.strip_prefix(SAVED_TAB)?;
            match poller.shared() {
                Some(shared) => TabRef::Doc(shared.spaces.tab(id)?.id.clone()),
                None => TabRef::File(id.parse().ok()?),
            }
        };
        Some((id, address))
    }

    /// Whether Sheprd keeps workspace `id` for this core.
    pub(in crate::hub) fn keeps_workspace(&self, id: &str) -> bool {
        lock(&self.poller).keeps(self.scope.as_deref(), id)
    }

    /// `scope`'s saved tab holding `terminal`, by which Sheprd knows the
    /// shared document's tab holding it.
    fn saved_tab_holding(&self, terminal: &str) -> Option<u64> {
        lock(&self.poller).saved_tab_holding(self.scope.as_deref()?, terminal)
    }

    /// The pane id of this core's terminal at `address`.
    fn pane_of(&self, address: &str) -> Option<String> {
        let state = lock(&self.state);
        Some(pane_id(&state.core.as_ref()?.hello().core.id, address))
    }
}

impl Hub {
    /// The saved tab Sheprd knows `tab` by, to ask it to change; `None`
    /// for a tab of the shared document no saved tab holds.
    pub(crate) fn desktop_tab(&self, tab: &TabRef, terminal: &str) -> Option<u64> {
        match tab {
            TabRef::File(id) | TabRef::Named { id, .. } => Some(*id),
            TabRef::Doc(_) => self.core.as_ref()?.saved_tab_holding(terminal),
        }
    }

    /// Whether `workspace` is one Sheprd keeps for Herdr 2, which only
    /// Sheprd renames.
    pub(crate) fn kept_workspace(&self, workspace: &str) -> bool {
        self.core
            .as_ref()
            .is_some_and(|core| core.keeps_workspace(workspace))
    }

    /// The pane of the core terminal at `address`, such as a split Sheprd
    /// made for a phone, once listed so the phone can open it; after a short
    /// wait it is answered anyway, and the phone finds it on reload.
    pub(crate) async fn core_pane_listed(&self, address: &str) -> Option<String> {
        let pane = self.core.as_ref()?.pane_of(address)?;
        let mut layout = self.layout.clone();
        let listed = layout.wait_for(|layout| layout.terminal(&pane).is_some());
        let _ = tokio::time::timeout(crate::hub::CORE_LISTING, listed).await;
        Some(pane)
    }
}
