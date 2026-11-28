//! The context an action runs with, from Herdr's own snapshot. Herdr fills
//! every field the context leaves out from its focused pane, so each case
//! compares the whole context: a missing field is a field Herdr would take
//! from another pane.

use crate::plugin_actions::{Error, Target, context::HostView};
use serde_json::{Value, json};

/// Two workspaces of two tabs each. Herdr's focus is on `w1:p1` in the
/// first; every target below is elsewhere. `w2:t1` holds two panes with the
/// second focused, and `w1:t2`'s pane has no agent, status, or worktree.
fn host() -> Value {
    json!({
        "workspaces": [
            {"workspace_id": "w1", "label": "api", "active_tab_id": "w1:t1",
             "worktree": {"key": "/src/api/.git", "label": "api", "is_linked_worktree": false}},
            {"workspace_id": "w2", "label": "docs", "active_tab_id": "w2:t2", "worktree": null},
        ],
        "tabs": [
            {"tab_id": "w1:t1", "workspace_id": "w1", "label": "server"},
            {"tab_id": "w1:t2", "workspace_id": "w1", "label": "tests"},
            {"tab_id": "w2:t1", "workspace_id": "w2", "label": "site"},
            {"tab_id": "w2:t2", "workspace_id": "w2", "label": "drafts"},
        ],
        "panes": [
            {"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1", "cwd": "/src/api",
             "agent": "claude", "agent_status": "working", "focused": true},
            {"pane_id": "w1:p2", "workspace_id": "w1", "tab_id": "w1:t2", "cwd": "/src/api/tests",
             "agent": null, "agent_status": null},
            {"pane_id": "w2:p1", "workspace_id": "w2", "tab_id": "w2:t1", "cwd": "/src/docs",
             "agent": "pi", "agent_status": "idle"},
            {"pane_id": "w2:p3", "workspace_id": "w2", "tab_id": "w2:t1", "cwd": "/src/docs/blog",
             "agent": "codex", "agent_status": "blocked"},
            {"pane_id": "w2:p2", "workspace_id": "w2", "tab_id": "w2:t2", "cwd": "/src/docs/drafts",
             "agent": "opencode", "agent_status": "done"},
        ],
        "layouts": [
            {"tab_id": "w1:t1", "focused_pane_id": "w1:p1"},
            {"tab_id": "w1:t2", "focused_pane_id": "w1:p2"},
            {"tab_id": "w2:t1", "focused_pane_id": "w2:p3"},
            {"tab_id": "w2:t2", "focused_pane_id": "w2:p2"},
        ],
    })
}

fn view(host: Value) -> HostView {
    serde_json::from_value(host).unwrap()
}

fn pane(id: &str) -> Target {
    Target::Pane(id.into())
}

/// A pane's context names that pane, its tab, and its workspace, whichever
/// pane Herdr has focused, and says it came from a context menu.
#[test]
fn a_pane_context_describes_that_pane_not_the_focused_one() {
    assert_eq!(
        view(host()).context(&pane("w2:p1"), None).unwrap(),
        json!({
            "workspace_id": "w2",
            "workspace_label": "docs",
            "workspace_cwd": "/src/docs",
            "tab_id": "w2:t1",
            "tab_label": "site",
            "focused_pane_id": "w2:p1",
            "focused_pane_cwd": "/src/docs",
            "focused_pane_agent": "pi",
            "focused_pane_status": "idle",
            "invocation_source": "context_menu",
        })
    );
    // The worktree record is Herdr's, passed back unchanged.
    assert_eq!(
        view(host()).context(&pane("w1:p2"), None).unwrap()["worktree"],
        json!({"key": "/src/api/.git", "label": "api", "is_linked_worktree": false})
    );
}

/// What the pane lacks is left out, never sent as `null`. Herdr then fills
/// a missing agent from its focused pane: its merge cannot say "none". This
/// pins that known gap, so an upstream change to it is noticed.
#[test]
fn absent_values_are_left_out_rather_than_null() {
    let context = view(host()).context(&pane("w1:p2"), None).unwrap();
    let object = context.as_object().unwrap();
    for key in ["focused_pane_agent", "focused_pane_status", "selected_text"] {
        assert!(!object.contains_key(key), "{key} in {context}");
    }
    assert!(object.values().all(|value| !value.is_null()), "{context}");
    let docs = view(host()).context(&pane("w2:p2"), None).unwrap();
    assert!(
        !docs.as_object().unwrap().contains_key("worktree"),
        "{docs}"
    );
}

/// The selection's text goes with the context only when it was read.
#[test]
fn selected_text_is_sent_only_when_read() {
    let context = view(host())
        .context(&pane("w2:p1"), Some("cargo test --locked".into()))
        .unwrap();
    assert_eq!(context["selected_text"], "cargo test --locked");
    assert_eq!(context["focused_pane_id"], "w2:p1");
}

/// A tab's context takes its pane fields from the pane focused in that tab,
/// not from the tab's first pane or Herdr's focused pane.
#[test]
fn a_tab_context_uses_the_pane_focused_in_that_tab() {
    assert_eq!(
        view(host())
            .context(&Target::Tab("w2:t1".into()), None)
            .unwrap(),
        json!({
            "workspace_id": "w2",
            "workspace_label": "docs",
            "workspace_cwd": "/src/docs/blog",
            "tab_id": "w2:t1",
            "tab_label": "site",
            "focused_pane_id": "w2:p3",
            "focused_pane_cwd": "/src/docs/blog",
            "focused_pane_agent": "codex",
            "focused_pane_status": "blocked",
            "invocation_source": "context_menu",
        })
    );
}

/// A workspace's context is its active tab's, and that tab's focused pane's.
#[test]
fn a_workspace_context_uses_its_active_tab() {
    assert_eq!(
        view(host())
            .context(&Target::Workspace("w2".into()), None)
            .unwrap(),
        json!({
            "workspace_id": "w2",
            "workspace_label": "docs",
            "workspace_cwd": "/src/docs/drafts",
            "tab_id": "w2:t2",
            "tab_label": "drafts",
            "focused_pane_id": "w2:p2",
            "focused_pane_cwd": "/src/docs/drafts",
            "focused_pane_agent": "opencode",
            "focused_pane_status": "done",
            "invocation_source": "context_menu",
        })
    );
}

/// A layout whose focused pane sits in another tab describes no pane of
/// this one, so the context names none.
#[test]
fn a_layout_naming_another_tabs_pane_is_ignored() {
    let mut host = host();
    host["layouts"][1]["focused_pane_id"] = json!("w2:p1");
    assert_eq!(
        view(host)
            .context(&Target::Tab("w1:t2".into()), None)
            .unwrap(),
        json!({
            "workspace_id": "w1",
            "workspace_label": "api",
            "worktree": {"key": "/src/api/.git", "label": "api", "is_linked_worktree": false},
            "tab_id": "w1:t2",
            "tab_label": "tests",
            "invocation_source": "context_menu",
        })
    );
}

/// A target Herdr no longer has, or whose tab moved to another workspace,
/// is gone rather than run somewhere else.
#[test]
fn a_target_missing_or_split_across_workspaces_is_gone() {
    let mut moved = host();
    // The pane still claims w2, but its tab now belongs to w1.
    moved["tabs"][2]["workspace_id"] = json!("w1");
    let mut orphan = host();
    orphan["tabs"].as_array_mut().unwrap().remove(1);
    for (host, target) in [
        (host(), pane("w9:p1")),
        (host(), Target::Tab("w9:t1".into())),
        (host(), Target::Workspace("w9".into())),
        (moved, pane("w2:p1")),
        (orphan, pane("w1:p2")),
    ] {
        assert!(
            matches!(view(host).context(&target, None), Err(Error::TargetGone)),
            "{target:?}"
        );
    }
}
