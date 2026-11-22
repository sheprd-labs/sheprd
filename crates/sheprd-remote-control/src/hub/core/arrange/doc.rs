//! A phone's change to a tab or workspace of the core's shared document
//! (`sheprd_session::spaces`), made by the daemon while Sheprd is closed:
//! written to the document as [`Edit`]s, which every client of the core
//! then reads. Each is made again over what another client wrote first.

use super::{Error, axis};
use crate::{
    control::layout::Op,
    hub::core::{HubError, Result, Source, TabRef, checkouts, lock},
};
use sheprd_backend::TerminalId;
use sheprd_core_client::{Core, spaces};
use sheprd_session::{
    WORKSPACE_LABEL,
    spaces::{Edit, Error as SpacesError, Spaces, Tab, TabEdit},
};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

/// How many times closing a workspace looks again for tabs another client
/// added to it while their terminals were ended.
const CLOSE_ATTEMPTS: usize = 4;

impl Source {
    /// Makes `op` at the terminal at `address` in the document's tab `tab`,
    /// or to the workspace it names. Answers a split's new terminal.
    pub(in crate::hub::core) fn arrange_doc(
        &self,
        tab: &TabRef,
        address: &str,
        op: &Op,
    ) -> Result<Option<String>> {
        let core = lock(&self.state)
            .core
            .clone()
            .ok_or(crate::hub::core::Error::Disconnected)?;
        match op {
            Op::RenameWorkspace { workspace, name } => {
                let edit = Edit::RenameWorkspace {
                    id: workspace.clone(),
                    label: name.trim().to_owned(),
                };
                spaces::update(&core, &[edit])?;
                return Ok(None);
            }
            Op::CloseWorkspace { workspace } => {
                self.close_workspace(&core, workspace)?;
                return Ok(None);
            }
            Op::RemoveWorktree { workspace, discard } => {
                self.remove_worktree(&core, workspace, *discard)?;
                return Ok(None);
            }
            _ => {}
        }
        let TabRef::Doc(id) = tab else {
            return Err(crate::hub::core::Error::Unsupported.into());
        };
        let at = TerminalId::new(address).map_err(Error::from)?;
        // The phone names a recovered chat's replacement, which a document
        // written before the recovery may not: the edit then names the pane
        // as the document does.
        let replacements = lock(&self.poller).replacements.clone();
        if let Some(axis) = axis(op) {
            let record = self.record(address)?;
            let workspace = lock(&self.poller)
                .shared()
                .and_then(|shared| shared.spaces.tab(id))
                .map(|tab| tab.workspace_id.clone());
            // Labelled for its workspace, so a client that has not read the
            // document yet still lists it there.
            let labels = workspace
                .as_deref()
                .filter(|id| sheprd_session::tab_workspace(id))
                .map(|id| BTreeMap::from([(WORKSPACE_LABEL, id)]))
                .unwrap_or_default();
            let new = self.start_beside(&record, labels)?;
            let split = TerminalId::new(new.address.as_str())
                .map_err(|error| HubError::from(Error::from(error)))
                .and_then(|new| {
                    // The split shows again, as the app's does.
                    edit_tab(&core, id, &at, &replacements, |_, pane| {
                        vec![
                            TabEdit::Split {
                                at: pane,
                                axis,
                                new: new.clone(),
                            },
                            TabEdit::Zoom { zoomed: None },
                        ]
                    })
                });
            if let Err(error) = split {
                // A write that timed out may still have landed: the shell
                // is ended only once a later read shows the tab without it.
                // The core answers one connection's requests in order, so
                // that read comes after the write it follows.
                match split_landed(&core, id, &new.address) {
                    Some(true) => return Ok(Some(new.address)),
                    Some(false) => self.end(&new.address),
                    None => {
                        tracing::warn!(%error, "A split's shell stayed: its tab could not be read")
                    }
                }
                return Err(error);
            }
            return Ok(Some(new.address));
        }
        match op {
            Op::Zoom => edit_tab(&core, id, &at, &replacements, |tab, pane| {
                let zoomed = tab.zoomed.is_none().then_some(pane);
                vec![TabEdit::Zoom { zoomed }]
            })?,
            Op::RenameTab(name) => edit_tab(&core, id, &at, &replacements, |_, _| {
                vec![TabEdit::Rename {
                    name: named(name.as_deref()),
                }]
            })?,
            Op::RenamePane(name) => edit_tab(&core, id, &at, &replacements, |_, pane| {
                vec![TabEdit::RenamePane {
                    terminal: pane,
                    name: named(name.as_deref()),
                }]
            })?,
            Op::Close => {
                spaces::update(&core, &[Edit::CloseTab { id: id.clone() }])?;
            }
            Op::SplitDown
            | Op::SplitRight
            | Op::RenameWorkspace { .. }
            | Op::CloseWorkspace { .. }
            | Op::RemoveWorktree { .. } => {
                return Err(crate::hub::core::Error::Unsupported.into());
            }
        }
        Ok(None)
    }

    /// Ends the terminals of `workspace`'s tabs, and those started for it,
    /// then forgets it, as its menu's Close does. A tab another client adds
    /// meanwhile has its terminals ended too before the workspace goes.
    fn close_workspace(&self, core: &Core, workspace: &str) -> Result<()> {
        let mut ended = HashSet::new();
        for attempt in 0..CLOSE_ATTEMPTS {
            let (spaces, _) = spaces::read(core)?;
            if attempt == 0 {
                editable(&spaces, workspace)?;
            }
            let mut addresses: Vec<String> = spaces
                .tabs_of(workspace)
                .flat_map(|tab| tab.panes.terminals())
                .map(|terminal| terminal.as_str().to_owned())
                .collect();
            addresses.extend(
                lock(&self.state)
                    .terminals
                    .iter()
                    .filter(|terminal| {
                        sheprd_session::labelled_workspace(&terminal.labels).is_some_and(
                            |labelled| labelled.session.is_none() && labelled.id == workspace,
                        )
                    })
                    .map(|terminal| terminal.address.clone()),
            );
            addresses.sort_unstable();
            addresses.dedup();
            addresses.retain(|address| !ended.contains(address));
            // A pane already closed stays in its tab until a desktop drops
            // it: a terminal the core no longer lists is ended already.
            let live: HashSet<String> = core
                .list(false)?
                .terminals
                .into_iter()
                .map(|terminal| terminal.address)
                .collect();
            let (addresses, gone): (Vec<_>, Vec<_>) = addresses
                .into_iter()
                .partition(|address| live.contains(address));
            ended.extend(gone);
            self.end_terminals(core, &addresses)?;
            ended.extend(addresses);
            let mut missed = false;
            spaces::change(core, |spaces| {
                missed = spaces
                    .tabs_of(workspace)
                    .flat_map(|tab| tab.panes.terminals())
                    .any(|terminal| !ended.contains(terminal.as_str()));
                if !missed {
                    spaces.apply(Edit::ForgetWorkspace {
                        id: workspace.to_owned(),
                    })?;
                }
                Ok(())
            })?;
            if !missed {
                return Ok(());
            }
        }
        Err(Error::Changed.into())
    }

    /// Deletes `workspace`'s linked worktree, then closes the workspace.
    /// A checkout with changes stays unless the person confirmed `discard`
    /// on the phone. The branch stays.
    fn remove_worktree(&self, core: &Core, workspace: &str, discard: bool) -> Result<()> {
        let (spaces, _) = spaces::read(core)?;
        // Nothing is deleted for a workspace the document would not forget.
        editable(&spaces, workspace)?;
        let cwd = spaces
            .workspace(workspace)
            .map(|workspace| workspace.cwd.clone())
            .filter(|cwd| !cwd.is_empty())
            .ok_or(Error::NotWorktree)?;
        let checkout = checkouts::read(Path::new(&cwd))
            .filter(|checkout| checkout.worktree.is_linked_worktree)
            .ok_or(Error::NotWorktree)?;
        // A workspace may be in a folder of the checkout: Git removes it by
        // its root.
        let root =
            crate::native::changes::local::root(Path::new(&cwd)).map_err(|_| Error::NotWorktree)?;
        let status = sheprd_client::new_command("git")
            .arg("--git-dir")
            .arg(&checkout.worktree.key)
            .args(remove_args(discard))
            .arg(&root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(Error::from)?;
        if !status.success() {
            return Err(Error::WorktreeKept.into());
        }
        self.close_workspace(core, workspace)
    }
}

/// Git's arguments before the checkout's root; `--force` discards its
/// uncommitted changes.
fn remove_args(discard: bool) -> &'static [&'static str] {
    if discard {
        &["worktree", "remove", "--force", "--"]
    } else {
        &["worktree", "remove", "--"]
    }
}

/// Makes the edits `change` answers to the document's tab `id`, at the
/// pane the phone names `at`, over whatever another client wrote first.
/// `change` sees the tab as last read, and the pane as the document names
/// it: a recovered chat's pane may still be named for the terminal it
/// replaced. A tab gone, or a pane it no longer holds, is refused and
/// leaves the document as it was.
fn edit_tab(
    core: &Core,
    id: &str,
    at: &TerminalId,
    replacements: &BTreeMap<TerminalId, TerminalId>,
    mut change: impl FnMut(&Tab, TerminalId) -> Vec<TabEdit>,
) -> Result<()> {
    let mut gone = false;
    spaces::change(core, |spaces| {
        gone = false;
        let Some(tab) = spaces.tab(id) else {
            return Err(SpacesError::Missing(id.to_owned()).into());
        };
        let Some(pane) = pane_named(tab, at, replacements) else {
            gone = true;
            return Ok(());
        };
        for edit in change(tab, pane) {
            spaces.apply(Edit::Tab {
                id: id.to_owned(),
                edit,
            })?;
        }
        Ok(())
    })?;
    if gone {
        return Err(Error::from(sheprd_session::panes::Error::Missing(at.clone())).into());
    }
    Ok(())
}

/// Whether the shared document's tab `id` holds the terminal at `address`
/// now; `None` when it cannot be read.
fn split_landed(core: &Core, id: &str, address: &str) -> Option<bool> {
    let terminal = TerminalId::new(address).ok()?;
    let (spaces, _) = spaces::read(core).ok()?;
    Some(
        spaces
            .tab(id)
            .is_some_and(|tab| tab.panes.contains(&terminal)),
    )
}

/// The pane of `tab` the phone names `at`: that terminal, or the one a
/// recovered chat replaced, as a document written before the recovery
/// still names it.
fn pane_named(
    tab: &Tab,
    at: &TerminalId,
    replacements: &BTreeMap<TerminalId, TerminalId>,
) -> Option<TerminalId> {
    if tab.panes.contains(at) {
        return Some(at.clone());
    }
    replacements
        .iter()
        .find(|(old, new)| *new == at && tab.panes.contains(old))
        .map(|(old, _)| old.clone())
}

/// Refuses a workspace the document does not keep, or a document this
/// version may not write, before anything is ended or deleted for it.
fn editable(spaces: &Spaces, workspace: &str) -> Result<()> {
    // Writing it refuses a newer format the way an edit would, naming it.
    if !spaces.writable() {
        spaces.to_value().map_err(sheprd_core_client::Error::from)?;
    }
    if spaces.workspace(workspace).is_none() {
        let missing = SpacesError::Missing(workspace.to_owned());
        return Err(sheprd_core_client::Error::from(missing).into());
    }
    Ok(())
}

/// A name given, or `None` for a blank one, which gives back the title.
fn named(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}
