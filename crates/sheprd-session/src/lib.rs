#![doc = include_str!("../README.md")]

mod error;
pub mod panes;
pub mod saved;
pub mod session;
pub mod spaces;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use error::{Error, Result};
pub use saved::{
    CoreTab, DEFAULT_WORKSPACE, Labelled, MAX_SAVED_BYTES, MAX_SAVED_TABS, NEW_WORKSPACE_LABEL,
    NEW_WORKSPACE_TITLE_LABEL, SESSION_LABEL, STAND_IN_SHELL, SavedTabs, SavedWorkspace,
    WORKSPACE_LABEL, labelled_workspace, new_workspace, new_workspace_title, tab_workspace,
};
pub use session::{Agent, CoreWorkspace, FolderStyle, Keeper, Session, Tab, Workspace, Worktree};
