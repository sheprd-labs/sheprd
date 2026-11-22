//! Adding what one client kept for a core before its document held it.

use super::{Adopted, Error, MAX_BYTES, MAX_ENTRIES, Spaces, Tab, Workspace, valid_id};
use serde::Serialize;

/// How many ids a generator is asked for before a record is left out. Ids
/// are random, so a second try is rare; the bound stops a broken generator
/// without taking its output.
const ID_TRIES: usize = 16;

impl Spaces {
    /// Adds what one client kept for this core before the document held it,
    /// as when the app first finds a core's document, without losing what
    /// is there:
    ///
    /// - a workspace whose folder a kept one has joins that one;
    /// - any other keeps its id when free, else takes one from `new_id`;
    /// - a tab whose terminals another tab already holds is left out, and
    ///   any other keeps its id when free, else takes one from `new_id`.
    ///
    /// A record is left out, with its tabs for a workspace, when it does not
    /// fit the bounds (entries or bytes) or `new_id` gives no free, valid
    /// id. Answers the ids each adopted record took, so what it answers is
    /// always written with the document.
    pub fn adopt(
        &mut self,
        workspaces: impl IntoIterator<Item = Workspace>,
        tabs: impl IntoIterator<Item = Tab>,
        mut new_id: impl FnMut() -> String,
    ) -> Result<Adopted, Error> {
        let mut size = serde_json::to_vec(&self.to_value()?)?.len();
        let mut adopted = Adopted::default();
        for workspace in workspaces {
            if !workspace.valid() {
                continue;
            }
            let joined = self
                .workspaces
                .iter()
                .find(|kept| !kept.cwd.is_empty() && kept.cwd == workspace.cwd)
                .map(|kept| kept.id.clone());
            if let Some(joined) = joined {
                adopted.workspaces.insert(workspace.id, joined);
                continue;
            }
            if self.workspaces.len() >= MAX_ENTRIES {
                continue;
            }
            let Some(id) = free_id(&workspace.id, &mut new_id, |id| {
                self.workspace(id).is_none()
            }) else {
                continue;
            };
            let known_as = workspace.id.clone();
            let workspace = Workspace { id, ..workspace };
            if !fits(&mut size, &workspace)? {
                continue;
            }
            adopted.workspaces.insert(known_as, workspace.id.clone());
            self.workspaces.push(workspace);
        }
        for tab in tabs {
            let Some(workspace_id) = adopted.workspaces.get(&tab.workspace_id).cloned() else {
                continue;
            };
            let held = tab
                .panes
                .terminals()
                .into_iter()
                .any(|terminal| self.tab_holding(terminal).is_some());
            if held || !tab.valid() || self.tabs.len() >= MAX_ENTRIES {
                continue;
            }
            let Some(id) = free_id(&tab.id, &mut new_id, |id| self.tab(id).is_none()) else {
                continue;
            };
            let known_as = tab.id.clone();
            let tab = Tab {
                id,
                workspace_id,
                ..tab
            };
            if !fits(&mut size, &tab)? {
                continue;
            }
            adopted.tabs.insert(known_as, tab.id.clone());
            self.tabs.push(tab);
        }
        Ok(adopted)
    }
}

/// `wanted` when `free`, else the first free, valid id `new_id` gives
/// within [`ID_TRIES`]; `None` when it gives none.
fn free_id(
    wanted: &str,
    new_id: &mut impl FnMut() -> String,
    free: impl Fn(&str) -> bool,
) -> Option<String> {
    if free(wanted) {
        return Some(wanted.to_owned());
    }
    (0..ID_TRIES)
        .map(|_| new_id())
        .find(|id| valid_id(id) && free(id))
}

/// Whether `record` still fits the document's bytes, counting it when it
/// does: its JSON and the comma before it in its list.
fn fits(size: &mut usize, record: &impl Serialize) -> Result<bool, Error> {
    let grown = *size + serde_json::to_vec(record)?.len() + 1;
    if grown > MAX_BYTES {
        return Ok(false);
    }
    *size = grown;
    Ok(true)
}
