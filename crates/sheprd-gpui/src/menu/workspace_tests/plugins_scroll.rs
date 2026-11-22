//! Keyboard scrolling in the workspace popover, which a window shorter than
//! the popover leaves with actions below or above its visible edge: the arrow
//! keys bring the selected action into view, whichever part of the popover
//! it sits in and however many plugin headings come before it.

use super::*;
use crate::plugin_actions::tests::fake;
use gpui::{TestAppContext, VisualTestContext, px, size};

/// Six plugins with two workspace actions each, so the popover is taller than
/// the window and headings sit between the plugin rows.
fn many_plugins() -> crate::plugin_actions::catalog::Catalog {
    let plugins = ["aider", "beads", "hunk", "notes", "review", "tasks"];
    let actions: Vec<_> = plugins
        .iter()
        .flat_map(|plugin| {
            ["first", "second"].map(|action| {
                serde_json::json!({"plugin_id": plugin, "action_id": action, "title": format!("{plugin} {action}"), "contexts": ["workspace"]})
            })
        })
        .collect();
    let installed: Vec<_> = plugins
        .iter()
        .map(|plugin| serde_json::json!({"plugin_id": plugin, "name": plugin, "enabled": true}))
        .collect();
    fake::catalog(serde_json::json!(actions), serde_json::json!(installed))
}

/// A window `height` tall with the popover open on `workspace` and the host's
/// answer landed. `pull_request` also shows the pull request card.
fn open_popover<'a>(
    cx: &'a mut TestAppContext,
    workspace: &'static str,
    height: f32,
    pull_request: bool,
) -> (gpui::Entity<SheprdWindow>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = sidebar::layout_tests::fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        fake::install(&mut view);
        view
    });
    cx.simulate_resize(size(px(800.), px(height)));
    fake::answer(many_plugins());
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_workspace_menu(workspace, Default::default(), window, cx);
            if pull_request {
                v.menu.github = crate::github::Auth::connected_fixture();
                v.menu.pr.value = Some(crate::pull_request::fixture().unwrap());
            }
        })
    });
    cx.run_until_parked();
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// The element the popover draws for `action`.
fn element(
    view: &gpui::Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    action: WorkspaceMenuAction,
) -> &'static str {
    let name = view.read_with(cx, |v, _| match action {
        WorkspaceMenuAction::Plugin(index) => {
            let row = &v.menu.target.as_ref().unwrap().plugins[index];
            format!("plugin-action-{}/{}", row.plugin_id, row.action_id)
        }
        WorkspaceMenuAction::PullRequest => "workspace-pr".to_owned(),
        action => {
            let (_, label) = v
                .workspace_items()
                .into_iter()
                .find(|(item, _)| *item == action)
                .unwrap();
            format!("workspace-menu-{}", label.english())
        }
    });
    name.leak()
}

/// Presses `key` once around the whole popover and once more, so it wraps,
/// and checks after each press that the selected action is drawn inside the
/// panel.
fn walk(view: &gpui::Entity<SheprdWindow>, cx: &mut VisualTestContext, key: &str, label: &str) {
    let count = view.read_with(cx, |v, _| v.workspace_menu_actions().len());
    for press in 1..=count + 1 {
        cx.simulate_keystrokes(key);
        draw(cx);
        let selected = view
            .read_with(cx, |v, _| v.menu.workspace_selected)
            .unwrap();
        let name = element(view, cx, selected);
        let panel = cx.debug_bounds("menu-panel").unwrap();
        let bounds = cx.debug_bounds(name).unwrap();
        assert!(
            bounds.top() >= panel.top() && bounds.bottom() <= panel.bottom(),
            "{label}: {key} press {press} of {}, on {selected:?}: {name} at {bounds:?} is outside the panel {panel:?}",
            count + 1
        );
    }
}

/// In a window shorter than the popover, each arrow key brings the action it
/// selects into view: plugin rows under any plugin's heading, the rows and
/// tiles above them, the header's mute toggle, and the pull request card,
/// including when a key wraps from the last action to the first or back.
#[gpui::test]
fn arrow_keys_keep_the_selected_workspace_action_in_view(cx: &mut TestAppContext) {
    for pull_request in [false, true] {
        let (view, cx) = open_popover(cx, "w3", 320., pull_request);
        draw(cx);
        let panel = cx.debug_bounds("menu-panel").unwrap();
        let last = cx.debug_bounds("plugin-action-tasks/second").unwrap();
        assert!(
            last.bottom() > panel.bottom(),
            "the popover fits: {panel:?}"
        );
        let label = if pull_request {
            "with the PR card"
        } else {
            "plugins only"
        };
        walk(&view, cx, "down", label);
        walk(&view, cx, "up", label);
    }
}

/// The popover reads top to bottom: the header with its mute toggle, the
/// tiles, the plain rows, the destructive row, each plugin's heading over its
/// rows, then the pull request card.
#[gpui::test]
fn the_popover_lists_its_own_rows_then_plugin_rows_then_the_pull_request(cx: &mut TestAppContext) {
    // `w4` is a linked worktree, which offers Delete worktree.
    let (view, cx) = open_popover(cx, "w4", 1600., true);
    draw(cx);
    let top = |cx: &mut VisualTestContext, name: &'static str| {
        cx.debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} not drawn"))
            .top()
    };
    let rows: Vec<&'static str> = view.read_with(cx, |v, _| {
        let items = v.workspace_items();
        let (_, rest) = crate::menu::workspace::popover::layout(&items);
        rest.into_iter()
            .map(|(_, label)| format!("workspace-menu-{}", label.english()).leak() as &str)
            .collect()
    });
    let delete = format!(
        "workspace-menu-{}",
        crate::menu::workspace::popover::Label::DELETE_WORKTREE.english()
    )
    .leak() as &str;
    assert!(rows.contains(&delete), "w4 offers no delete: {rows:?}");
    assert!(rows.len() > 1, "w4 has no plain row: {rows:?}");
    let plain: Vec<&str> = rows.iter().copied().filter(|row| *row != delete).collect();

    let mut order = vec!["workspace-menu-header", "workspace-menu-tiles"];
    order.extend(plain);
    order.extend([
        delete,
        "plugin-heading-aider",
        "plugin-action-aider/first",
        "plugin-action-aider/second",
        "plugin-heading-beads",
        "plugin-action-tasks/second",
        "workspace-pr",
    ]);
    let tops: Vec<_> = order.iter().map(|name| top(cx, name)).collect();
    for pair in order.windows(2).zip(tops.windows(2)) {
        assert!(
            pair.1[0] < pair.1[1],
            "{} is not above {}: {tops:?}",
            pair.0[0],
            pair.0[1]
        );
    }
    let header = cx.debug_bounds("workspace-menu-header").unwrap();
    let mute = format!(
        "workspace-menu-{}",
        crate::menu::workspace::popover::Label::MUTE.english()
    )
    .leak() as &str;
    let mute = cx.debug_bounds(mute).unwrap();
    assert!(header.contains(&mute.origin), "mute is not in the header");
}
