use super::super::diff::edits;
use super::*;
use crate::browser::shared::View;
use crate::core_terminals::panes::Axis;
use sheprd_session::spaces::{Tab, TabEdit, Workspace};

fn workspace(id: &str) -> Workspace {
    Workspace {
        id: id.to_owned(),
        label: id.to_owned(),
        custom: false,
        cwd: String::new(),
    }
}

fn shared_tab(tab: &str, workspace: &str, terminal: &str) -> Tab {
    Tab {
        id: tab.to_owned(),
        workspace_id: workspace.to_owned(),
        title: String::new(),
        name: None,
        names: Default::default(),
        panes: Panes::single(id(terminal)),
        zoomed: None,
    }
}

fn view(workspaces: &[&str], tabs: Vec<Tab>) -> View {
    View {
        tabs,
        workspaces: workspaces.iter().map(|id| workspace(id)).collect(),
    }
}

#[test]
fn nothing_changed_is_no_edit() {
    let base = view(&["a", "b"], vec![shared_tab("t", "a", "t1")]);
    assert!(edits(&base, &base.clone()).is_empty());
}

#[test]
fn new_and_gone_records_are_kept_put_closed_and_forgotten() {
    let base = view(&["a", "b"], vec![shared_tab("t", "b", "t1")]);
    let local = view(&["a", "c"], vec![shared_tab("u", "c", "t2")]);
    assert_eq!(
        edits(&base, &local),
        [
            Edit::KeepWorkspace(workspace("c")),
            Edit::CloseTab { id: "t".into() },
            Edit::OpenTab(shared_tab("u", "c", "t2")),
            Edit::ForgetWorkspace { id: "b".into() },
        ]
    );
}

/// Without a folder, an automatic name is not written: it follows the
/// folder ([`a_folder_followed_here_reaches_the_document`]).
#[test]
fn only_a_name_the_user_gave_is_written() {
    let base = view(&["a"], Vec::new());
    let mut local = base.clone();
    local.workspaces[0].label = "named".into();
    assert!(edits(&base, &local).is_empty());
    local.workspaces[0].custom = true;
    assert_eq!(
        edits(&base, &local),
        [Edit::RenameWorkspace {
            id: "a".into(),
            label: "named".into()
        }]
    );
}

#[test]
fn a_new_order_moves_each_workspace_to_its_place() {
    let base = view(&["a", "b", "c"], Vec::new());
    let local = view(&["c", "a", "b"], Vec::new());
    let made = edits(&base, &local);
    let mut spaces = Spaces::default();
    for id in ["a", "b", "c"] {
        spaces.apply(Edit::KeepWorkspace(workspace(id))).unwrap();
    }
    for edit in made {
        spaces.apply(edit).unwrap();
    }
    let order: Vec<_> = spaces.workspaces().iter().map(|w| w.id.as_str()).collect();
    assert_eq!(order, ["c", "a", "b"]);
}

#[test]
fn a_title_alone_is_not_a_change() {
    let base = view(&["a"], vec![shared_tab("t", "a", "t1")]);
    let mut local = base.clone();
    local.tabs[0].title = "vim".into();
    assert!(edits(&base, &local).is_empty());
    local.tabs[0].name = Some("editor".into());
    assert_eq!(
        edits(&base, &local),
        [Edit::Tab {
            id: "t".into(),
            edit: TabEdit::Rename {
                name: Some("editor".into())
            }
        }]
    );
}

fn tab_edit(id: &str, edit: TabEdit) -> Edit {
    Edit::Tab {
        id: id.to_owned(),
        edit,
    }
}

#[test]
fn each_change_to_a_tab_is_its_own_edit() {
    let base = view(&["a", "b"], vec![shared_tab("t", "a", "t1")]);
    let mut local = base.clone();
    let tab = &mut local.tabs[0];
    tab.workspace_id = "b".into();
    tab.panes.split(&id("t1"), Axis::Column, id("t2")).unwrap();
    tab.names.insert("t2".into(), "logs".into());
    tab.zoomed = Some(id("t2"));
    assert_eq!(
        edits(&base, &local),
        [
            tab_edit(
                "t",
                TabEdit::Move {
                    workspace_id: "b".into()
                }
            ),
            tab_edit(
                "t",
                TabEdit::Split {
                    at: id("t1"),
                    axis: Axis::Column,
                    new: id("t2"),
                }
            ),
            tab_edit(
                "t",
                TabEdit::RenamePane {
                    terminal: id("t2"),
                    name: Some("logs".into()),
                }
            ),
            tab_edit(
                "t",
                TabEdit::Zoom {
                    zoomed: Some(id("t2"))
                }
            ),
        ]
    );
}

#[test]
fn a_closed_pane_is_removed_and_a_new_layout_is_set_whole() {
    let mut split = Panes::single(id("t1"));
    split.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    let mut tab = shared_tab("t", "a", "t1");
    tab.panes = split.clone();
    let base = view(&["a"], vec![tab]);
    let mut local = base.clone();
    local.tabs[0].panes = Panes::single(id("t1"));
    assert_eq!(
        edits(&base, &local),
        [tab_edit("t", TabEdit::RemovePane { terminal: id("t2") })]
    );
    let mut swapped = split.clone();
    assert!(swapped.swap(&id("t1"), &id("t2")));
    local.tabs[0].panes = swapped.clone();
    assert_eq!(
        edits(&base, &local),
        [tab_edit("t", TabEdit::SetPanes { panes: swapped })]
    );
}

#[test]
fn a_split_here_and_a_rename_elsewhere_both_stay() {
    let base = view(&["a"], vec![shared_tab("t", "a", "t1")]);
    let mut spaces = Spaces::default();
    spaces.apply(Edit::KeepWorkspace(workspace("a"))).unwrap();
    spaces
        .apply(Edit::OpenTab(shared_tab("t", "a", "t1")))
        .unwrap();
    // Another client renames the tab; this one splits it.
    spaces
        .apply(tab_edit(
            "t",
            TabEdit::Rename {
                name: Some("api".into()),
            },
        ))
        .unwrap();
    let mut local = base.clone();
    local.tabs[0]
        .panes
        .split(&id("t1"), Axis::Row, id("t2"))
        .unwrap();
    for edit in edits(&base, &local) {
        spaces.apply(edit).unwrap();
    }
    let tab = spaces.tab("t").unwrap();
    assert_eq!(tab.name.as_deref(), Some("api"));
    assert_eq!(tab.panes, local.tabs[0].panes);
}

fn document(view: &View) -> Spaces {
    let mut spaces = Spaces::default();
    for workspace in &view.workspaces {
        spaces
            .apply(Edit::KeepWorkspace(workspace.clone()))
            .unwrap();
    }
    for tab in &view.tabs {
        spaces.apply(Edit::OpenTab(tab.clone())).unwrap();
    }
    spaces
}

#[test]
fn a_pane_moved_to_another_tab_is_let_go_before_it_lands() {
    // The only pane of the second tab moves into the first, listed before
    // it; then a pane of a split moves the other way.
    let base = view(
        &["a"],
        vec![shared_tab("one", "a", "t1"), shared_tab("two", "a", "t2")],
    );
    let mut local = view(&["a"], vec![shared_tab("one", "a", "t1")]);
    local.tabs[0]
        .panes
        .split(&id("t1"), Axis::Row, id("t2"))
        .unwrap();
    let mut spaces = document(&base);
    for edit in edits(&base, &local) {
        spaces.apply(edit).unwrap();
    }
    assert_eq!(spaces.tab("one").unwrap().panes, local.tabs[0].panes);
    assert!(spaces.tab("two").is_none());

    let base = local;
    let mut local = base.clone();
    local.tabs[0].panes = Panes::single(id("t1"));
    local.tabs.insert(0, shared_tab("three", "a", "t2"));
    let mut spaces = document(&base);
    for edit in edits(&base, &local) {
        spaces.apply(edit).unwrap();
    }
    assert_eq!(spaces.tab("one").unwrap().panes, Panes::single(id("t1")));
    assert_eq!(spaces.tab("three").unwrap().panes, Panes::single(id("t2")));
}

#[test]
fn panes_opened_and_closed_are_splits_and_removals_never_a_new_set() {
    let base = view(&["a"], vec![shared_tab("t", "a", "t1")]);
    // The only pane replaced: it closes once its replacement is there.
    let mut local = base.clone();
    local.tabs[0].panes = Panes::single(id("t2"));
    assert_eq!(
        edits(&base, &local),
        [
            tab_edit(
                "t",
                TabEdit::Split {
                    at: id("t1"),
                    axis: Axis::Row,
                    new: id("t2"),
                }
            ),
            tab_edit("t", TabEdit::RemovePane { terminal: id("t1") }),
        ]
    );
    // Two panes at once, the second placed by a layout over the same
    // terminals.
    let mut local = base.clone();
    let panes = &mut local.tabs[0].panes;
    panes.split(&id("t1"), Axis::Column, id("t2")).unwrap();
    panes.split(&id("t1"), Axis::Row, id("t3")).unwrap();
    for local in [local, {
        let mut swapped = base.clone();
        swapped.tabs[0].panes = Panes::single(id("t9"));
        swapped
    }] {
        let mut spaces = document(&base);
        for edit in edits(&base, &local) {
            assert!(
                !matches!(&edit, Edit::Tab { edit: TabEdit::SetPanes { panes }, .. }
                    if panes.count() != local.tabs[0].panes.count()),
                "{edit:?}"
            );
            spaces.apply(edit).unwrap();
        }
        assert_eq!(spaces.tab("t").unwrap().panes, local.tabs[0].panes);
    }
}

/// The document as `base` holds it, with `local`'s edits written over it.
fn written(base: &View, local: &View) -> Spaces {
    let mut spaces = Spaces::default();
    for workspace in &base.workspaces {
        spaces
            .apply(Edit::KeepWorkspace(workspace.clone()))
            .unwrap();
    }
    for edit in edits(base, local) {
        spaces.apply(edit).unwrap();
    }
    spaces
}

/// Other clients, such as a phone, read a workspace's folder from the
/// document: its checkout, branch, and pull request follow from it. A folder
/// the workspace followed here must reach them, whoever named it.
#[test]
fn a_folder_followed_here_reaches_the_document() {
    let mut base = view(&["a", "b"], Vec::new());
    base.workspaces[0].cwd = "/old".into();
    base.workspaces[1].cwd = "/old".into();
    base.workspaces[1].custom = true;
    let mut local = base.clone();
    local.workspaces[0].cwd = "/new".into();
    local.workspaces[1].cwd = "/new".into();
    let spaces = written(&base, &local);
    assert_eq!(spaces.workspace("a").unwrap().cwd, "/new");
    assert_eq!(spaces.workspace("b").unwrap().cwd, "/new");
}

/// A workspace the user has not named is named after its folder here: other
/// clients show the name the document holds, so it must follow too, and
/// stay a name the user did not give.
#[test]
fn an_automatic_name_followed_here_reaches_the_document() {
    let mut base = view(&["a"], Vec::new());
    base.workspaces[0].cwd = "/work/old".into();
    let mut local = base.clone();
    local.workspaces[0].cwd = "/work/new".into();
    local.workspaces[0].label = "new".into();
    let spaces = written(&base, &local);
    let workspace = spaces.workspace("a").unwrap();
    assert_eq!(workspace.label, "new");
    assert!(!workspace.custom);
}
