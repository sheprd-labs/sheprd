//! Opening a tab, and the changes one tab takes, each touching only its
//! own fields: a rename made on one computer and a zoom on another both
//! stay.

use super::{Error, MAX_ENTRIES, Spaces, Tab};
use crate::panes::{Axis, Panes};
use sheprd_backend::TerminalId;

/// One change to one tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabEdit {
    /// Moves it to another workspace.
    Move { workspace_id: String },
    /// Closes the pane `terminal`; the last pane stays, as a tab always
    /// shows one.
    RemovePane { terminal: TerminalId },
    /// Its name; `None` gives back its program's title.
    Rename { name: Option<String> },
    /// The name of the pane `terminal`; `None` gives back its title.
    RenamePane {
        terminal: TerminalId,
        name: Option<String>,
    },
    /// Its program's title, shown when it has no name.
    Retitle { title: String },
    /// Its whole layout, for what moves panes without adding or closing
    /// one (resizing, swapping). Refused as [`Error::Stale`] when the tab
    /// no longer holds the same terminals, as after another client's split
    /// or closed pane: writing it would undo that change.
    SetPanes { panes: Panes },
    /// `new` beside the pane `at`: right of it on [`Axis::Row`], below on
    /// [`Axis::Column`].
    Split {
        at: TerminalId,
        axis: Axis,
        new: TerminalId,
    },
    /// The pane filling it, or `None` to show its split again. A pane it
    /// no longer holds shows its split.
    Zoom { zoomed: Option<TerminalId> },
}

impl Spaces {
    pub(super) fn open_tab(&mut self, tab: Tab) -> Result<(), Error> {
        if !tab.valid() {
            return Err(Error::Invalid);
        }
        if self.workspace(&tab.workspace_id).is_none() {
            return Err(Error::Missing(tab.workspace_id));
        }
        if self.tab(&tab.id).is_some() {
            return Err(Error::Taken(tab.id));
        }
        if let Some(held) = tab
            .panes
            .terminals()
            .into_iter()
            .find(|terminal| self.tab_holding(terminal).is_some())
        {
            return Err(Error::Held(held.clone()));
        }
        if self.tabs.len() >= MAX_ENTRIES {
            return Err(Error::Full);
        }
        self.tabs.push(tab);
        Ok(())
    }

    /// Makes `edit` to the tab `id`, on a copy checked before it replaces
    /// the tab, so a refused edit changes nothing.
    pub(super) fn edit_tab(&mut self, id: &str, edit: TabEdit) -> Result<(), Error> {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return Ok(());
        };
        let mut tab = self.tabs[index].clone();
        match edit {
            TabEdit::Move { workspace_id } => {
                if self.workspace(&workspace_id).is_none() {
                    return Err(Error::Missing(workspace_id));
                }
                tab.workspace_id = workspace_id;
            }
            TabEdit::RemovePane { terminal } => {
                tab.panes.remove(&terminal);
                tab.forget_gone_panes();
            }
            TabEdit::Rename { name } => {
                tab.name = name.filter(|name| !name.trim().is_empty());
            }
            TabEdit::RenamePane { terminal, name } => {
                if !tab.panes.contains(&terminal) {
                    return Ok(());
                }
                match name.filter(|name| !name.trim().is_empty()) {
                    Some(name) => tab.names.insert(terminal.as_str().to_owned(), name),
                    None => tab.names.remove(terminal.as_str()),
                };
            }
            TabEdit::Retitle { title } => tab.title = title,
            TabEdit::SetPanes { panes } => {
                if !same_terminals(&tab.panes, &panes) {
                    return Err(Error::Stale(tab.id));
                }
                tab.panes = panes;
            }
            TabEdit::Split { at, axis, new } => {
                if self.tab_holding(&new).is_some() {
                    return Err(Error::Held(new));
                }
                tab.panes.split(&at, axis, new)?;
            }
            TabEdit::Zoom { zoomed } => {
                tab.zoomed = zoomed.filter(|zoomed| tab.panes.contains(zoomed));
            }
        }
        if !tab.valid() {
            return Err(Error::Invalid);
        }
        self.tabs[index] = tab;
        Ok(())
    }
}

/// Whether two layouts hold the same terminals, wherever they are.
fn same_terminals(held: &Panes, laid_out: &Panes) -> bool {
    let mut held = held.terminals();
    let mut laid_out = laid_out.terminals();
    held.sort_unstable();
    laid_out.sort_unstable();
    held == laid_out
}

impl Tab {
    /// Drops the names and zoom of panes the tab no longer holds.
    fn forget_gone_panes(&mut self) {
        let panes = &self.panes;
        self.names.retain(|terminal, _| {
            TerminalId::new(terminal).is_ok_and(|terminal| panes.contains(&terminal))
        });
        if self
            .zoomed
            .as_ref()
            .is_some_and(|zoomed| !panes.contains(zoomed))
        {
            self.zoomed = None;
        }
    }
}
