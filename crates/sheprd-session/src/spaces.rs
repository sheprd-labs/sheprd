//! The workspaces and tabs Sheprd keeps for one herdr-core, held in that
//! core's [`DOC`] document so every client of the core reads and changes
//! the same ones: the app on the core's own computer, the app on another
//! computer that reaches it over SSH, and the remote control daemon.
//!
//! The document holds what every client shows alike: which workspaces
//! there are, their names, folders and order, and each tab's terminals and
//! split layout. What one window shows (which tab is in front, the selected
//! pane, a tab watching another's terminal) stays with that window.
//!
//! Clients change the document with [`Edit`]s applied to what they last
//! read and written back over that version, so two clients editing at once
//! each keep their change: the later write is refused, and it applies its
//! edit again to the newer document (`sheprd_core_client::spaces`).

mod adopt;
mod error;
mod tabs;

pub use error::Error;
pub use tabs::TabEdit;

use crate::panes::Panes;
use crate::saved::{MAX_CWD_BYTES, MAX_ID_BYTES, MAX_LABEL_CHARS, MAX_SAVED_TABS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sheprd_backend::TerminalId;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The herdr-core document holding a core's [`Spaces`].
pub const DOC: &str = "sheprd.spaces";
/// The format this version writes. A document of a newer format is read
/// for what this version knows, and never written.
pub const FORMAT: u32 = 1;
/// The largest document read or written.
pub const MAX_BYTES: usize = 256 * 1024;
/// The most workspaces, and the most tabs, a document holds.
pub const MAX_ENTRIES: usize = MAX_SAVED_TABS;
/// How a workspace id the document hands out starts, apart from Herdr 1's
/// and from the `space-N` ids the app kept in its own file.
pub const ID_PREFIX: &str = "space-";

/// One core's workspaces and tabs, in the user's order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spaces {
    #[serde(default)]
    format: u32,
    #[serde(default, deserialize_with = "readable")]
    workspaces: Vec<Workspace>,
    #[serde(default, deserialize_with = "readable")]
    tabs: Vec<Tab>,
}

/// A workspace Sheprd keeps for the core's terminals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Unique in the document; tabs name it.
    pub id: String,
    pub label: String,
    /// Whether the user named it; otherwise it is named for its folder, and
    /// a linked worktree's row shows its branch instead.
    #[serde(default)]
    pub custom: bool,
    /// Where its new terminals start, on the core's computer; empty for none.
    #[serde(default)]
    pub cwd: String,
}

/// A tab of the core's terminals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    /// Unique in the document, and never reused: clients map it to tabs of
    /// their own.
    pub id: String,
    pub workspace_id: String,
    /// Its program's title when the document was last written.
    #[serde(default)]
    pub title: String,
    /// The name the user gave it, shown over its title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Names the user gave its panes, by terminal address.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    pub panes: Panes,
    /// The pane filling it, when one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed: Option<TerminalId>,
}

/// One change to a document. Each names only what it changes, so two
/// clients changing different things at once both keep their change when
/// the later one applies its edit again to the newer document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Closes a tab; one already gone is no change.
    CloseTab { id: String },
    /// Moves a workspace to the folder its terminals are in. A workspace the
    /// user has not named also takes `label`, the name that folder gives it;
    /// one the user named keeps its name. One already gone is no change.
    FollowWorkspace {
        id: String,
        cwd: String,
        label: String,
    },
    /// Forgets a workspace and every tab in it; one already gone is no
    /// change.
    ForgetWorkspace { id: String },
    /// Keeps a new workspace at the end. An id already kept is refused.
    KeepWorkspace(Workspace),
    /// Moves a workspace before another, or to the end when `before` is
    /// `None` or gone.
    MoveWorkspace { id: String, before: Option<String> },
    /// Adds a new tab at the end. An id already held is refused, and so is
    /// a terminal another tab holds.
    OpenTab(Tab),
    /// Renames a workspace, which the user then named.
    RenameWorkspace { id: String, label: String },
    /// Changes one thing about the tab `id`. A tab already gone is no
    /// change: an edit never brings back a tab another client closed.
    Tab { id: String, edit: TabEdit },
}

/// What [`Spaces::adopt`] changed: the document ids the adopted workspaces
/// and tabs took, by the ids the caller knew them by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adopted {
    pub tabs: HashMap<String, String>,
    pub workspaces: HashMap<String, String>,
}

impl Spaces {
    /// The document's `value` as `doc.get` answers it: `Null` for one never
    /// written. A record that fails to read or breaks a bound is left out
    /// rather than costing the rest.
    pub fn from_value(value: Value) -> Result<Self, Error> {
        if value.is_null() {
            return Ok(Self::default());
        }
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Large);
        }
        let mut spaces: Self = serde_json::from_value(value)?;
        spaces.workspaces.retain(Workspace::valid);
        spaces.workspaces.truncate(MAX_ENTRIES);
        let mut seen = HashSet::new();
        spaces
            .workspaces
            .retain(|workspace| seen.insert(workspace.id.clone()));
        let kept: HashSet<&str> = spaces.workspaces.iter().map(|w| w.id.as_str()).collect();
        let mut tab_ids = HashSet::new();
        let tabs = std::mem::take(&mut spaces.tabs)
            .into_iter()
            .filter(|tab| {
                tab.valid()
                    && kept.contains(tab.workspace_id.as_str())
                    && tab_ids.insert(tab.id.clone())
            })
            .take(MAX_ENTRIES)
            .collect();
        spaces.tabs = tabs;
        Ok(spaces)
    }

    /// The value to write with `doc.set`, refused when this version cannot
    /// write the document's format or it outgrew its bounds.
    pub fn to_value(&self) -> Result<Value, Error> {
        if self.format > FORMAT {
            return Err(Error::Newer(self.format));
        }
        let mut written = self.clone();
        written.format = FORMAT;
        let value = serde_json::to_value(&written)?;
        if serde_json::to_vec(&value)?.len() > MAX_BYTES {
            return Err(Error::Large);
        }
        Ok(value)
    }

    /// Whether this version may write the document.
    pub fn writable(&self) -> bool {
        self.format <= FORMAT
    }

    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    /// Every tab, in the order they were opened.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab(&self, id: &str) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    /// `workspace`'s tabs, in the order they were opened.
    pub fn tabs_of<'a>(&'a self, workspace: &'a str) -> impl Iterator<Item = &'a Tab> + 'a {
        self.tabs
            .iter()
            .filter(move |tab| tab.workspace_id == workspace)
    }

    /// The tab holding `terminal`, if any.
    pub fn tab_holding(&self, terminal: &TerminalId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.panes.contains(terminal))
    }

    /// Makes `edit`, keeping every bound; on an error nothing changed.
    pub fn apply(&mut self, edit: Edit) -> Result<(), Error> {
        if !self.writable() {
            return Err(Error::Newer(self.format));
        }
        match edit {
            Edit::CloseTab { id } => self.tabs.retain(|tab| tab.id != id),
            Edit::FollowWorkspace { id, cwd, label } => {
                let label = clean_label(&label);
                let Some(workspace) = self
                    .workspaces
                    .iter_mut()
                    .find(|workspace| workspace.id == id)
                else {
                    return Ok(());
                };
                let followed = Workspace {
                    cwd,
                    label: if workspace.custom {
                        workspace.label.clone()
                    } else {
                        label
                    },
                    ..workspace.clone()
                };
                if !followed.valid() {
                    return Err(Error::Invalid);
                }
                *workspace = followed;
            }
            Edit::ForgetWorkspace { id } => {
                self.workspaces.retain(|workspace| workspace.id != id);
                self.tabs.retain(|tab| tab.workspace_id != id);
            }
            Edit::KeepWorkspace(workspace) => {
                if !workspace.valid() {
                    return Err(Error::Invalid);
                }
                if self.workspace(&workspace.id).is_some() {
                    return Err(Error::Taken(workspace.id));
                }
                if self.workspaces.len() >= MAX_ENTRIES {
                    return Err(Error::Full);
                }
                self.workspaces.push(workspace);
            }
            Edit::MoveWorkspace { id, before } => {
                let Some(from) = self.workspaces.iter().position(|w| w.id == id) else {
                    return Err(Error::Missing(id));
                };
                // Before itself is where it is.
                if before.as_deref() == Some(id.as_str()) {
                    return Ok(());
                }
                let moved = self.workspaces.remove(from);
                let to = before
                    .and_then(|before| self.workspaces.iter().position(|w| w.id == before))
                    .unwrap_or(self.workspaces.len());
                self.workspaces.insert(to, moved);
            }
            Edit::OpenTab(tab) => self.open_tab(tab)?,
            Edit::Tab { id, edit } => self.edit_tab(&id, edit)?,
            Edit::RenameWorkspace { id, label } => {
                let label = clean_label(&label);
                if label.is_empty() || label.chars().count() > MAX_LABEL_CHARS {
                    return Err(Error::Invalid);
                }
                let workspace = self
                    .workspaces
                    .iter_mut()
                    .find(|workspace| workspace.id == id)
                    .ok_or(Error::Missing(id))?;
                workspace.label = label;
                workspace.custom = true;
            }
        }
        Ok(())
    }
}

impl Workspace {
    /// Whether a workspace is one a client could have written.
    pub fn valid(&self) -> bool {
        valid_id(&self.id)
            && !self.label.trim().is_empty()
            && self.label.chars().count() <= MAX_LABEL_CHARS
            && self.cwd.len() <= MAX_CWD_BYTES
            && (self.cwd.is_empty() || !self.cwd.contains('\0'))
    }
}

impl Tab {
    /// Whether a tab is one a client could have written: bounded names, and
    /// a zoomed pane that is one of its own.
    pub fn valid(&self) -> bool {
        valid_id(&self.id)
            && valid_id(&self.workspace_id)
            && self.title.len() <= MAX_ID_BYTES * 4
            && self
                .name
                .as_ref()
                .is_none_or(|name| name.len() <= MAX_ID_BYTES * 4)
            && self.names.len() <= MAX_ENTRIES
            && self.names.iter().all(|(terminal, name)| {
                terminal.len() <= MAX_ID_BYTES && name.len() <= MAX_ID_BYTES * 4
            })
            && self
                .zoomed
                .as_ref()
                .is_none_or(|zoomed| self.panes.contains(zoomed))
    }
}

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_BYTES && !id.chars().any(char::is_control)
}

/// A name on one line, without the spaces around it.
fn clean_label(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The records of a list that read, each on its own.
fn readable<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let values = Vec::<Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}
