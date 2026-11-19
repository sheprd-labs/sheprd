//! Plugin actions in the workspace popover, against a stand-in host: the
//! popover lists workspace actions after its own rows, the arrow keys reach
//! them in that order, and a row runs on the popover's workspace, or closes
//! the popover when that workspace is gone.

use super::*;
use crate::i18n::Localize;
use crate::plugin_actions::{Target, tests::fake};
use gpui::{TestAppContext, VisualTestContext};

fn window(cx: &mut TestAppContext) -> (gpui::Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = sidebar::layout_tests::fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        fake::install(&mut view);
        view
    });
    (view, cx)
}

/// Opens the popover on `w3`, which is not the focused workspace, and lets
/// the host's answer land.
fn open(view: &gpui::Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_workspace_menu("w3", Default::default(), window, cx);
        })
    });
    cx.run_until_parked();
}

fn selected(
    view: &gpui::Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
) -> Option<WorkspaceMenuAction> {
    view.read_with(cx, |v, _| v.menu.workspace_selected)
}

/// The arrow keys walk the popover's own rows, then its plugin rows; Enter
/// on one runs that action on the popover's workspace.
#[gpui::test]
fn workspace_plugin_rows_follow_the_popovers_rows_and_run_on_its_workspace(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx);
    fake::answer(fake::hunk_and_notes());
    open(&view, cx);
    let actions = view.read_with(cx, |v, _| v.workspace_menu_actions());
    let first = actions
        .iter()
        .position(|action| *action == WorkspaceMenuAction::Plugin(0))
        .unwrap();
    assert!(
        actions[..first]
            .iter()
            .all(|action| !matches!(action, WorkspaceMenuAction::Plugin(_)))
    );
    assert_eq!(
        actions[first..],
        [
            WorkspaceMenuAction::Plugin(0),
            WorkspaceMenuAction::Plugin(1)
        ]
    );
    let titles = view.read_with(cx, |v, _| {
        v.menu
            .target
            .as_ref()
            .unwrap()
            .plugins
            .iter()
            .map(|row| row.action_id.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(titles, ["review-workspace", "workspace-log"]);

    for _ in 0..first {
        cx.simulate_keystrokes("down");
    }
    assert_eq!(selected(&view, cx), Some(actions[first - 1]));
    cx.simulate_keystrokes("down down");
    assert_eq!(selected(&view, cx), Some(WorkspaceMenuAction::Plugin(1)));
    cx.simulate_keystrokes("enter");
    let run = fake::take_invocations();
    assert_eq!(run.len(), 1);
    assert_eq!(run[0].action_id, "workspace-log");
    assert_eq!(run[0].target, Target::Workspace("w3".into()));
    assert!(view.read_with(cx, |v, _| v.menu.page.is_none()));

    // Hovering selects the row Enter then runs.
    open(&view, cx);
    cx.update(|window, cx| sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let row = cx
        .debug_bounds("plugin-action-hunk/review-workspace")
        .unwrap();
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::default());
    assert_eq!(selected(&view, cx), Some(WorkspaceMenuAction::Plugin(0)));
    cx.simulate_keystrokes("enter");
    assert_eq!(fake::take_run(), ["hunk/review-workspace"]);
}

/// An answer that changes the rows while the popover is open drops a
/// selected plugin row, which may now be another action.
#[gpui::test]
fn a_late_answer_clears_a_selected_workspace_plugin_row(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    fake::answer(fake::hunk_and_notes());
    open(&view, cx);
    fake::answer(fake::catalog(
        serde_json::json!([
            {"plugin_id": "aider", "action_id": "ask", "title": "Ask aider", "contexts": ["workspace"]},
            {"plugin_id": "hunk", "action_id": "review-workspace", "title": "Review workspace in Hunk", "contexts": ["workspace"]},
        ]),
        serde_json::json!([
            {"plugin_id": "aider", "name": "Aider", "enabled": true},
            {"plugin_id": "hunk", "name": "Hunk", "enabled": true},
        ]),
    ));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_workspace_menu("w3", Default::default(), window, cx);
        });
        // Up from nothing selects the last row: a plugin row of the last answer.
        window.dispatch_keystroke(crate::keymap::parse_keystroke("up").unwrap(), cx);
    });
    assert_eq!(selected(&view, cx), Some(WorkspaceMenuAction::Plugin(1)));
    cx.run_until_parked();
    assert_eq!(selected(&view, cx), None);
    assert_eq!(
        view.read_with(cx, |v, _| v.menu.target.as_ref().unwrap().plugins[0]
            .action_id
            .clone()),
        "ask"
    );
}

/// A plugin row of a popover opened before its workspace closed, the daemon
/// restarted, or the connection changed closes the popover and says so,
/// and sends nothing.
#[gpui::test]
fn a_workspace_plugin_row_refuses_once_its_workspace_is_gone(cx: &mut TestAppContext) {
    for change in ["closed", "restarted", "reconnected"] {
        let (view, cx) = window(cx);
        fake::answer(fake::hunk_and_notes());
        open(&view, cx);
        view.update(cx, |v, _| {
            let snapshot = std::sync::Arc::make_mut(v.live.snapshot.as_mut().unwrap());
            match change {
                "closed" => snapshot.workspaces.retain(|w| w.workspace_id != "w3"),
                "restarted" => snapshot.boot_id.push_str("-restarted"),
                _ => v.endpoints[0].generation += 1,
            }
        });
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.activate_workspace_menu(WorkspaceMenuAction::Plugin(0), window, cx)
            })
        });
        cx.run_until_parked();
        assert!(fake::take_run().is_empty(), "{change}");
        view.read_with(cx, |v, _| {
            assert!(v.menu.page.is_none(), "{change}");
            let (flash, _) = v.flash.as_ref().unwrap();
            assert_eq!(
                *flash,
                crate::window::Flash::warning(crate::Error::StaleWorkspace.localize()),
                "{change}"
            );
        });
    }
}
