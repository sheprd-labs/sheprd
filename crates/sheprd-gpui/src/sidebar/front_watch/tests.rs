#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::fixture_window;
use std::sync::Arc;

fn rows(ids: &[(usize, &str)]) -> Vec<WorkspaceRow> {
    ids.iter()
        .map(|(host, id)| (*host, (*id).to_owned()))
        .collect()
}

#[test]
fn the_row_above_takes_the_place_or_the_row_below_when_first() {
    let rows = rows(&[(0, "main"), (0, "a"), (1, "elsewhere"), (0, "b"), (0, "c")]);
    assert_eq!(successors(&rows, 0, "c"), ["b", "a", "main"]);
    // Another host's row between them never counts.
    assert_eq!(successors(&rows, 0, "b"), ["a", "main", "c"]);
    assert_eq!(successors(&rows, 0, "main"), ["a", "b", "c"]);
    assert!(successors(&rows, 1, "elsewhere").is_empty());
    assert!(successors(&rows, 0, "missing").is_empty());
}

#[gpui::test]
fn removing_the_workspace_in_front_brings_the_row_above_forward(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            let rows = view.sidebar_workspaces(cx);
            // The last row, so the daemon's own pick would differ.
            let (_, last) = rows.iter().rfind(|(host, _)| *host == 0).unwrap().clone();
            let above = successors(&rows, 0, &last)[0].clone();
            snapshot.focused_workspace_id = Some(last.clone());
            view.live.snapshot = Some(Arc::new(snapshot.clone()));
            view.follow_removed_front(cx);
            assert!(view.pending_navigation.is_none());

            // The daemon removes it and focuses its first workspace.
            snapshot
                .workspaces
                .retain(|workspace| workspace.workspace_id != last);
            let first = snapshot.workspaces[0].workspace_id.clone();
            assert_ne!(first, above);
            snapshot.focused_workspace_id = Some(first);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.follow_removed_front(cx);
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Workspace(above))
            );
        })
    });
}

#[gpui::test]
fn choosing_another_workspace_moves_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            let first = snapshot.workspaces[0].workspace_id.clone();
            let second = snapshot.workspaces[1].workspace_id.clone();
            snapshot.focused_workspace_id = Some(first);
            view.live.snapshot = Some(Arc::new(snapshot.clone()));
            view.follow_removed_front(cx);
            snapshot.focused_workspace_id = Some(second);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.follow_removed_front(cx);
            assert!(view.pending_navigation.is_none());
        })
    });
}

#[gpui::test]
fn rows_removed_together_land_on_the_nearest_survivor_above(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            let rows = view.sidebar_workspaces(cx);
            let (_, last) = rows.iter().rfind(|(host, _)| *host == 0).unwrap().clone();
            let above = successors(&rows, 0, &last);
            snapshot.focused_workspace_id = Some(last.clone());
            view.live.snapshot = Some(Arc::new(snapshot.clone()));
            view.follow_removed_front(cx);

            // A sweep removes it and the row above, its progress dialog
            // open: nothing moves until the dialog closes.
            snapshot.workspaces.retain(|workspace| {
                workspace.workspace_id != last && workspace.workspace_id != above[0]
            });
            snapshot.focused_workspace_id = Some(snapshot.workspaces[0].workspace_id.clone());
            view.live.snapshot = Some(Arc::new(snapshot));
            view.menu.page = Some(Page::RemoveMerged);
            view.follow_removed_front(cx);
            assert!(view.pending_navigation.is_none());
            view.menu.page = None;
            view.follow_removed_front(cx);
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Workspace(above[1].clone()))
            );
        })
    });
}
