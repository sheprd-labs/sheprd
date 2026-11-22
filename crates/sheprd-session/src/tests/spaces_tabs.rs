//! Changes to one shared tab keep what another client changed meanwhile,
//! and adopting stays within the document's bounds.

use super::spaces::{counter, tab, terminal, with, workspace};
use crate::panes::Axis;
use crate::spaces::{Edit, Error, MAX_BYTES, Spaces, TabEdit};

fn edit(id: &str, edit: TabEdit) -> Edit {
    Edit::Tab {
        id: id.to_owned(),
        edit,
    }
}

fn one_tab() -> Spaces {
    with([
        Edit::KeepWorkspace(workspace("space-a", "")),
        Edit::KeepWorkspace(workspace("space-b", "")),
        Edit::OpenTab(tab("tab-1", "space-a", "t1")),
    ])
}

#[test]
fn a_rename_and_a_zoom_made_at_once_both_stay() {
    let mut spaces = one_tab();
    spaces
        .apply(edit(
            "tab-1",
            TabEdit::Split {
                at: terminal("t1"),
                axis: Axis::Row,
                new: terminal("t2"),
            },
        ))
        .unwrap();
    // As `update` does: the second client's edit made again to the first's
    // document.
    spaces
        .apply(edit(
            "tab-1",
            TabEdit::Rename {
                name: Some("build".to_owned()),
            },
        ))
        .unwrap();
    spaces
        .apply(edit(
            "tab-1",
            TabEdit::Zoom {
                zoomed: Some(terminal("t2")),
            },
        ))
        .unwrap();
    let tab = spaces.tab("tab-1").unwrap();
    assert_eq!(tab.name.as_deref(), Some("build"));
    assert_eq!(tab.zoomed, Some(terminal("t2")));
    assert!(tab.panes.contains(&terminal("t1")) && tab.panes.contains(&terminal("t2")));
}

#[test]
fn an_edit_to_a_closed_tab_does_not_bring_it_back() {
    let mut spaces = one_tab();
    spaces
        .apply(Edit::CloseTab {
            id: "tab-1".to_owned(),
        })
        .unwrap();
    spaces
        .apply(edit("tab-1", TabEdit::Rename { name: None }))
        .unwrap();
    assert!(spaces.tabs().is_empty());
}

#[test]
fn opening_refuses_a_taken_id_or_a_terminal_another_tab_holds() {
    let mut spaces = one_tab();
    assert!(matches!(
        spaces.apply(Edit::OpenTab(tab("tab-1", "space-a", "t9"))),
        Err(Error::Taken(_))
    ));
    assert!(matches!(
        spaces.apply(Edit::OpenTab(tab("tab-2", "space-a", "t1"))),
        Err(Error::Held(_))
    ));
}

#[test]
fn closing_a_pane_forgets_its_name_and_zoom() {
    let mut spaces = one_tab();
    for change in [
        TabEdit::Split {
            at: terminal("t1"),
            axis: Axis::Column,
            new: terminal("t2"),
        },
        TabEdit::RenamePane {
            terminal: terminal("t2"),
            name: Some("logs".to_owned()),
        },
        TabEdit::Zoom {
            zoomed: Some(terminal("t2")),
        },
        TabEdit::RemovePane {
            terminal: terminal("t2"),
        },
    ] {
        spaces.apply(edit("tab-1", change)).unwrap();
    }
    let tab = spaces.tab("tab-1").unwrap();
    assert!(!tab.panes.contains(&terminal("t2")));
    assert!(tab.names.is_empty());
    assert_eq!(tab.zoomed, None);
}

#[test]
fn a_split_or_layout_taking_another_tabs_terminal_is_refused_and_changes_nothing() {
    let mut spaces = one_tab();
    spaces
        .apply(Edit::OpenTab(tab("tab-2", "space-a", "t2")))
        .unwrap();
    let before = spaces.clone();
    let split = TabEdit::Split {
        at: terminal("t1"),
        axis: Axis::Row,
        new: terminal("t2"),
    };
    assert!(matches!(
        spaces.apply(edit("tab-1", split)),
        Err(Error::Held(_))
    ));
    let taken = tab("x", "space-a", "t2").panes;
    assert!(matches!(
        spaces.apply(edit("tab-1", TabEdit::SetPanes { panes: taken })),
        Err(Error::Stale(_))
    ));
    assert_eq!(spaces, before);
}

/// `tab-1` split into `t1` and `t2`.
fn split_tab() -> Spaces {
    let mut spaces = one_tab();
    let split = TabEdit::Split {
        at: terminal("t1"),
        axis: Axis::Row,
        new: terminal("t2"),
    };
    spaces.apply(edit("tab-1", split)).unwrap();
    spaces
}

#[test]
fn a_layout_moving_the_same_panes_is_taken() {
    let mut spaces = split_tab();
    let mut swapped = spaces.tab("tab-1").unwrap().panes.clone();
    assert!(swapped.swap(&terminal("t1"), &terminal("t2")));
    spaces
        .apply(edit(
            "tab-1",
            TabEdit::SetPanes {
                panes: swapped.clone(),
            },
        ))
        .unwrap();
    assert_eq!(spaces.tab("tab-1").unwrap().panes, swapped);
}

#[test]
fn a_layout_read_before_another_clients_split_or_close_is_refused() {
    // Read before the split: it would drop the new pane.
    let before_split = one_tab().tab("tab-1").unwrap().panes.clone();
    let mut spaces = split_tab();
    let kept = spaces.clone();
    assert!(matches!(
        spaces.apply(edit(
            "tab-1",
            TabEdit::SetPanes {
                panes: before_split
            }
        )),
        Err(Error::Stale(_))
    ));
    assert_eq!(spaces, kept);
    // Read before a pane closed: it would bring the pane back.
    let mut swapped = spaces.tab("tab-1").unwrap().panes.clone();
    swapped.swap(&terminal("t1"), &terminal("t2"));
    spaces
        .apply(edit(
            "tab-1",
            TabEdit::RemovePane {
                terminal: terminal("t2"),
            },
        ))
        .unwrap();
    assert!(matches!(
        spaces.apply(edit("tab-1", TabEdit::SetPanes { panes: swapped })),
        Err(Error::Stale(_))
    ));
    assert!(!spaces.tab("tab-1").unwrap().panes.contains(&terminal("t2")));
}

#[test]
fn a_tab_moves_only_to_a_kept_workspace() {
    let mut spaces = one_tab();
    let to = |workspace: &str| TabEdit::Move {
        workspace_id: workspace.to_owned(),
    };
    spaces.apply(edit("tab-1", to("space-b"))).unwrap();
    assert_eq!(spaces.tab("tab-1").unwrap().workspace_id, "space-b");
    assert!(matches!(
        spaces.apply(edit("tab-1", to("gone"))),
        Err(Error::Missing(_))
    ));
}

#[test]
fn adopting_stops_at_the_byte_bound_and_answers_only_what_was_kept() {
    let mut spaces = Spaces::default();
    let workspaces = [workspace("space-1", "/code/app")];
    let tabs: Vec<_> = (0..256)
        .map(|index| {
            let mut tab = tab(&format!("tab-{index}"), "space-1", &format!("t{index}"));
            tab.title = "x".repeat(1024);
            tab
        })
        .collect();
    let adopted = spaces.adopt(workspaces, tabs, counter()).unwrap();
    assert!(adopted.tabs.len() < 256, "some tabs left out");
    assert_eq!(adopted.tabs.len(), spaces.tabs().len());
    let value = spaces.to_value().unwrap();
    assert!(serde_json::to_vec(&value).unwrap().len() <= MAX_BYTES);
}

#[test]
fn adopting_leaves_out_a_record_no_free_valid_id_is_found_for() {
    let mut spaces = with([Edit::KeepWorkspace(workspace("space-1", "/code/app"))]);
    // A broken generator: always a taken id, then an invalid one.
    let adopted = spaces
        .adopt([workspace("space-1", "/code/other")], [], || {
            "space-1".to_owned()
        })
        .unwrap();
    assert!(adopted.workspaces.is_empty());
    let adopted = spaces
        .adopt([workspace("space-1", "/code/other")], [], String::new)
        .unwrap();
    assert!(adopted.workspaces.is_empty());
    assert_eq!(spaces.workspaces().len(), 1);
}
