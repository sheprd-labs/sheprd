//! The invocation context a plugin action runs with, for the pane, tab, or
//! workspace a menu was opened on.
//!
//! Herdr fills every field a caller leaves out from its *focused* pane
//! (`merge_plugin_context`), and a right-clicked pane is often not the
//! focused one. So the context names every field Herdr would derive for the
//! target itself, read from Herdr's own `session.snapshot` taken just before
//! the action runs: the GUI's snapshot has no worktree paths and no focused
//! pane per tab. Herdr derives a tab's or workspace's context from the pane
//! focused in that tab, and a workspace's from its active tab, so this does.

use super::{Error, Result};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// What `invocation_source` tells a plugin about where it was run from.
const SOURCE: &str = "context_menu";

/// The pane, tab, or workspace an action runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Workspace(String),
    Tab(String),
    Pane(String),
}

/// The parts of Herdr's `session.snapshot` a context is built from. Values
/// Herdr reads back unchanged (the worktree record, the agent status) stay
/// as Herdr wrote them.
#[derive(Debug, Deserialize)]
pub(crate) struct HostView {
    workspaces: Vec<Workspace>,
    tabs: Vec<Tab>,
    panes: Vec<Pane>,
    #[serde(default)]
    layouts: Vec<Layout>,
}

#[derive(Debug, Deserialize)]
struct Workspace {
    workspace_id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    active_tab_id: Option<String>,
    #[serde(default)]
    worktree: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Tab {
    tab_id: String,
    workspace_id: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Pane {
    pane_id: String,
    workspace_id: String,
    tab_id: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    agent_status: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Layout {
    tab_id: String,
    #[serde(default)]
    focused_pane_id: Option<String>,
}

impl HostView {
    fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.workspace_id == id)
    }

    fn tab(&self, id: &str) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.tab_id == id)
    }

    fn pane(&self, id: &str) -> Option<&Pane> {
        self.panes.iter().find(|p| p.pane_id == id)
    }

    /// The pane focused in tab `id`, as Herdr's layout records it.
    fn focused_in(&self, id: &str) -> Option<&Pane> {
        let pane = self
            .layouts
            .iter()
            .find(|layout| layout.tab_id == id)?
            .focused_pane_id
            .as_deref()?;
        self.pane(pane).filter(|pane| pane.tab_id == id)
    }

    /// The context Herdr would build for `target`, with `selected_text`
    /// when the action takes the pane's selection.
    pub(crate) fn context(&self, target: &Target, selected_text: Option<String>) -> Result<Value> {
        let (workspace, tab, pane) = match target {
            Target::Pane(id) => {
                let pane = self.pane(id).ok_or(Error::TargetGone)?;
                let tab = self.tab(&pane.tab_id).ok_or(Error::TargetGone)?;
                (pane.workspace_id.as_str(), Some(tab), Some(pane))
            }
            Target::Tab(id) => {
                let tab = self.tab(id).ok_or(Error::TargetGone)?;
                (tab.workspace_id.as_str(), Some(tab), self.focused_in(id))
            }
            Target::Workspace(id) => {
                let workspace = self.workspace(id).ok_or(Error::TargetGone)?;
                let tab = workspace
                    .active_tab_id
                    .as_deref()
                    .and_then(|id| self.tab(id));
                let pane = tab.and_then(|tab| self.focused_in(&tab.tab_id));
                (id.as_str(), tab, pane)
            }
        };
        let workspace = self.workspace(workspace).ok_or(Error::TargetGone)?;
        if tab.is_some_and(|tab| tab.workspace_id != workspace.workspace_id) {
            return Err(Error::TargetGone);
        }
        let mut context = Map::new();
        let mut set = |key: &str, value: Option<Value>| {
            if let Some(value) = value {
                context.insert(key.to_owned(), value);
            }
        };
        let text = |value: Option<&String>| value.map(|value| json!(value));
        set("workspace_id", Some(json!(workspace.workspace_id)));
        set("workspace_label", text(workspace.label.as_ref()));
        // Herdr's own workspace directory is its focused pane's.
        set(
            "workspace_cwd",
            text(pane.and_then(|pane| pane.cwd.as_ref())),
        );
        set("worktree", workspace.worktree.clone());
        set("tab_id", tab.map(|tab| json!(tab.tab_id)));
        set("tab_label", text(tab.and_then(|tab| tab.label.as_ref())));
        set("focused_pane_id", pane.map(|pane| json!(pane.pane_id)));
        set(
            "focused_pane_cwd",
            text(pane.and_then(|pane| pane.cwd.as_ref())),
        );
        set(
            "focused_pane_agent",
            text(pane.and_then(|pane| pane.agent.as_ref())),
        );
        set(
            "focused_pane_status",
            pane.and_then(|pane| pane.agent_status.clone()),
        );
        set("selected_text", selected_text.map(Value::String));
        set("invocation_source", Some(json!(SOURCE)));
        Ok(Value::Object(context))
    }
}
