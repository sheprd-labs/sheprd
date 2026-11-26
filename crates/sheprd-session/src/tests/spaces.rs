use crate::panes::Panes;
use crate::spaces::{Edit, Error, FORMAT, MAX_BYTES, MAX_ENTRIES, Spaces, Tab, Workspace};
use serde_json::{Value, json};
use sheprd_backend::TerminalId;

pub(super) fn terminal(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

pub(super) fn workspace(id: &str, cwd: &str) -> Workspace {
    Workspace {
        id: id.to_owned(),
        label: id.to_owned(),
        custom: false,
        cwd: cwd.to_owned(),
    }
}

pub(super) fn tab(id: &str, workspace: &str, terminal_name: &str) -> Tab {
    Tab {
        id: id.to_owned(),
        workspace_id: workspace.to_owned(),
        title: "zsh".to_owned(),
        name: None,
        names: Default::default(),
        panes: Panes::single(terminal(terminal_name)),
        zoomed: None,
    }
}

pub(super) fn with(edits: impl IntoIterator<Item = Edit>) -> Spaces {
    let mut spaces = Spaces::default();
    for edit in edits {
        spaces.apply(edit).unwrap();
    }
    spaces
}

/// A generator handing out `new-1`, `new-2`, …
pub(super) fn counter() -> impl FnMut() -> String {
    let mut next = 0;
    move || {
        next += 1;
        format!("new-{next}")
    }
}

#[test]
fn a_document_never_written_holds_nothing() {
    let spaces = Spaces::from_value(Value::Null).unwrap();
    assert!(spaces.workspaces().is_empty() && spaces.tabs().is_empty());
    assert!(spaces.writable());
}

#[test]
fn what_is_written_reads_back_the_same() {
    let spaces = with([
        Edit::KeepWorkspace(workspace("space-a", "/code/app")),
        Edit::OpenTab(tab("tab-1", "space-a", "t1")),
    ]);
    let value = spaces.to_value().unwrap();
    assert_eq!(value["format"], FORMAT);
    let read = Spaces::from_value(value).unwrap();
    assert_eq!(read.workspaces(), spaces.workspaces());
    assert_eq!(read.tabs(), spaces.tabs());
}

#[test]
fn a_damaged_record_costs_only_itself() {
    let value = json!({
        "format": 1,
        "workspaces": [
            {"id": "space-a", "label": "app", "cwd": "/code/app"},
            {"id": "", "label": "no id"},
            {"label": "missing id"},
            {"id": "space-a", "label": "twice"},
        ],
        "tabs": [
            {"id": "tab-1", "workspace_id": "space-a", "panes": "t1"},
            {"id": "tab-2", "workspace_id": "gone", "panes": "t2"},
            {"id": "tab-3", "workspace_id": "space-a"},
            {"id": "tab-1", "workspace_id": "space-a", "panes": "t3"},
        ],
    });
    let spaces = Spaces::from_value(value).unwrap();
    let ids: Vec<_> = spaces
        .workspaces()
        .iter()
        .map(|w| w.label.as_str())
        .collect();
    assert_eq!(ids, ["app"]);
    let tabs: Vec<_> = spaces.tabs().iter().map(|tab| tab.id.as_str()).collect();
    assert_eq!(tabs, ["tab-1"]);
}

#[test]
fn an_oversized_document_is_refused() {
    let value = json!({"workspaces": [], "padding": "x".repeat(MAX_BYTES)});
    assert!(matches!(Spaces::from_value(value), Err(Error::Large)));
}

#[test]
fn a_newer_format_is_read_but_never_written() {
    let value = json!({"format": FORMAT + 1, "workspaces": [
        {"id": "space-a", "label": "app", "future": true}
    ]});
    let mut spaces = Spaces::from_value(value).unwrap();
    assert_eq!(spaces.workspaces().len(), 1);
    assert!(!spaces.writable());
    assert!(matches!(spaces.to_value(), Err(Error::Newer(_))));
    let edit = Edit::ForgetWorkspace {
        id: "space-a".to_owned(),
    };
    assert!(matches!(spaces.apply(edit), Err(Error::Newer(_))));
    assert_eq!(spaces.workspaces().len(), 1, "nothing changed");
}

#[test]
fn forgetting_a_workspace_closes_its_tabs() {
    let mut spaces = with([
        Edit::KeepWorkspace(workspace("space-a", "/code/a")),
        Edit::KeepWorkspace(workspace("space-b", "/code/b")),
        Edit::OpenTab(tab("tab-1", "space-a", "t1")),
        Edit::OpenTab(tab("tab-2", "space-b", "t2")),
    ]);
    spaces
        .apply(Edit::ForgetWorkspace {
            id: "space-a".to_owned(),
        })
        .unwrap();
    assert!(spaces.workspace("space-a").is_none());
    assert!(spaces.tab("tab-1").is_none());
    assert!(spaces.tab("tab-2").is_some());
    // Already gone: no change, no error, as a second client's retry finds.
    spaces
        .apply(Edit::ForgetWorkspace {
            id: "space-a".to_owned(),
        })
        .unwrap();
}

#[test]
fn a_workspace_moves_before_another_or_to_the_end() {
    let mut spaces = with(["a", "b", "c"].map(|id| Edit::KeepWorkspace(workspace(id, ""))));
    let order = |spaces: &Spaces| {
        spaces
            .workspaces()
            .iter()
            .map(|w| w.id.clone())
            .collect::<Vec<_>>()
    };
    let move_to = |id: &str, before: Option<&str>| Edit::MoveWorkspace {
        id: id.to_owned(),
        before: before.map(str::to_owned),
    };
    spaces.apply(move_to("c", Some("a"))).unwrap();
    assert_eq!(order(&spaces), ["c", "a", "b"]);
    spaces.apply(move_to("c", None)).unwrap();
    assert_eq!(order(&spaces), ["a", "b", "c"]);
    spaces.apply(move_to("a", Some("gone"))).unwrap();
    assert_eq!(order(&spaces), ["b", "c", "a"]);
    spaces.apply(move_to("c", Some("c"))).unwrap();
    assert_eq!(
        order(&spaces),
        ["b", "c", "a"],
        "before itself is where it is"
    );
    assert!(matches!(
        spaces.apply(move_to("gone", None)),
        Err(Error::Missing(_))
    ));
}

#[test]
fn renaming_names_the_workspace_as_the_user_did() {
    let mut spaces = with([Edit::KeepWorkspace(workspace("space-a", "/code/a"))]);
    let rename = |label: &str| Edit::RenameWorkspace {
        id: "space-a".to_owned(),
        label: label.to_owned(),
    };
    spaces.apply(rename("  Review\n")).unwrap();
    let renamed = spaces.workspace("space-a").unwrap();
    assert_eq!(renamed.label, "Review");
    assert!(renamed.custom);
    assert!(matches!(spaces.apply(rename("   ")), Err(Error::Invalid)));
}

#[test]
fn following_moves_the_folder_and_renames_only_an_unnamed_workspace() {
    let mut spaces = with([
        Edit::KeepWorkspace(workspace("space-a", "/code/a")),
        Edit::KeepWorkspace(workspace("space-b", "/code/b")),
    ]);
    spaces
        .apply(Edit::RenameWorkspace {
            id: "space-b".to_owned(),
            label: "Mine".to_owned(),
        })
        .unwrap();
    let follow = |id: &str, cwd: &str, label: &str| Edit::FollowWorkspace {
        id: id.to_owned(),
        cwd: cwd.to_owned(),
        label: label.to_owned(),
    };
    spaces.apply(follow("space-a", "/code/c", " c\n")).unwrap();
    spaces.apply(follow("space-b", "/code/c", "c")).unwrap();
    let unnamed = spaces.workspace("space-a").unwrap();
    assert_eq!(
        (unnamed.cwd.as_str(), unnamed.label.as_str()),
        ("/code/c", "c")
    );
    assert!(!unnamed.custom);
    let named = spaces.workspace("space-b").unwrap();
    assert_eq!(
        (named.cwd.as_str(), named.label.as_str()),
        ("/code/c", "Mine")
    );
    assert!(named.custom);

    assert!(matches!(
        spaces.apply(follow("space-a", "/x", "  ")),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        spaces.apply(follow("space-a", "/x\0y", "x")),
        Err(Error::Invalid)
    ));
    // Another client may have closed it meanwhile: following never fails
    // the edits beside it.
    spaces.apply(follow("gone", "/x", "x")).unwrap();
    assert!(spaces.workspace("gone").is_none());
    assert_eq!(spaces.workspace("space-a").unwrap().cwd, "/code/c");
}

#[test]
fn a_tab_needs_its_workspace_and_a_zoomed_pane_of_its_own() {
    let mut spaces = with([Edit::KeepWorkspace(workspace("space-a", ""))]);
    assert!(matches!(
        spaces.apply(Edit::OpenTab(tab("tab-1", "gone", "t1"))),
        Err(Error::Missing(_))
    ));
    let mut zoomed = tab("tab-1", "space-a", "t1");
    zoomed.zoomed = Some(terminal("elsewhere"));
    assert!(matches!(
        spaces.apply(Edit::OpenTab(zoomed)),
        Err(Error::Invalid)
    ));
}

#[test]
fn keeping_refuses_a_taken_id_and_past_the_bound() {
    let mut spaces = with([Edit::KeepWorkspace(workspace("space-a", ""))]);
    assert!(matches!(
        spaces.apply(Edit::KeepWorkspace(workspace("space-a", "/other"))),
        Err(Error::Taken(_))
    ));
    for index in 1..MAX_ENTRIES {
        spaces
            .apply(Edit::KeepWorkspace(workspace(&format!("w{index}"), "")))
            .unwrap();
    }
    assert!(matches!(
        spaces.apply(Edit::KeepWorkspace(workspace("one-more", ""))),
        Err(Error::Full)
    ));
}

#[test]
fn adopting_joins_a_folder_already_kept_and_renames_a_taken_id() {
    let mut spaces = with([
        Edit::KeepWorkspace(workspace("space-1", "/code/app")),
        Edit::OpenTab(tab("tab-1", "space-1", "t1")),
    ]);
    let adopted = spaces
        .adopt(
            [
                // This client's `space-1` is another folder: it takes a new id.
                workspace("space-1", "/code/other"),
                // Its `space-2` is the folder the document keeps as `space-1`.
                workspace("space-2", "/code/app"),
            ],
            [
                tab("tab-1", "space-1", "t5"),
                tab("tab-2", "space-2", "t6"),
                // The document already holds `t1`.
                tab("tab-3", "space-2", "t1"),
            ],
            counter(),
        )
        .unwrap();
    assert_eq!(adopted.workspaces["space-1"], "new-1");
    assert_eq!(adopted.workspaces["space-2"], "space-1");
    assert_eq!(adopted.tabs["tab-1"], "new-2");
    assert_eq!(adopted.tabs["tab-2"], "tab-2");
    assert!(!adopted.tabs.contains_key("tab-3"));
    let other = spaces.workspace("new-1").unwrap();
    assert_eq!(other.cwd, "/code/other");
    assert_eq!(spaces.tab("new-2").unwrap().workspace_id, "new-1");
    assert_eq!(spaces.tab("tab-2").unwrap().workspace_id, "space-1");
    assert_eq!(spaces.tabs().len(), 3);
}

#[test]
fn adopting_twice_adds_nothing_the_second_time() {
    let mut spaces = Spaces::default();
    let workspaces = [workspace("space-1", "/code/app")];
    let tabs = [tab("tab-1", "space-1", "t1")];
    spaces
        .adopt(workspaces.clone(), tabs.clone(), counter())
        .unwrap();
    let once = spaces.clone();
    let adopted = spaces.adopt(workspaces, tabs, counter()).unwrap();
    assert_eq!(spaces, once);
    assert_eq!(adopted.workspaces["space-1"], "space-1");
    assert!(adopted.tabs.is_empty());
}
