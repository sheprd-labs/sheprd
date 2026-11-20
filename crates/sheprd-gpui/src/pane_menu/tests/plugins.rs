//! Plugin actions in the pane menu, against a stand-in host: which rows the
//! menu lists and when, which pane a row runs on, the keys and pointer over
//! built-in and plugin rows together, and what a late answer or a failure
//! does to an open menu.

use super::*;
use crate::plugin_actions::{
    Target as PluginTarget,
    catalog::Catalog,
    tests::{bridge_script::FakeHerdr, fake},
};
use gpui::VisualTestContext;

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        fake::install(&mut view);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

fn open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, pane: &str) {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_pane_menu(pane, point(px(100.), px(40.)), window, cx);
            assert_eq!(v.menu.page, Some(Page::Pane));
        })
    });
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// A key, handled without running the background work it may wait on.
fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|window, cx| {
        window.dispatch_keystroke(crate::keymap::parse_keystroke(key).unwrap(), cx)
    });
}

/// `plugin/action` of the open menu's plugin rows.
fn rows(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| {
        v.menu
            .pane
            .as_ref()
            .unwrap()
            .plugins
            .iter()
            .map(|row| format!("{}/{}", row.plugin_id, row.action_id))
            .collect()
    })
}

/// The open menu's own row count and its keyboard selection.
fn selection(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> (usize, Option<usize>) {
    view.read_with(cx, |v, _| {
        let pane = v.menu.pane.as_ref().unwrap();
        (pane.actions().len(), pane.selected)
    })
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    draw(cx);
    let selector: &'static str = selector.to_owned().leak();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} not drawn"));
    cx.simulate_click(bounds.center(), Modifiers::default());
}

/// Hunk and Notes, plus one more plugin, so the rows differ from
/// [`fake::hunk_and_notes`] by where they sit.
fn with_aider(title: &str) -> Catalog {
    fake::catalog(
        json!([
            {"plugin_id": "aider", "action_id": "ask", "title": title, "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "review-pane", "title": "Review changes in Hunk", "contexts": ["pane"]},
            {"plugin_id": "notes", "action_id": "append", "title": "Append to notes", "contexts": ["pane", "selection"]},
        ]),
        json!([
            {"plugin_id": "aider", "name": "Aider", "enabled": true},
            {"plugin_id": "hunk", "name": "Hunk", "enabled": true},
            {"plugin_id": "notes", "name": "Notes", "enabled": true},
        ]),
    )
}

/// The headline case: a right-clicked pane that is not focused. The menu
/// opens at once with its own rows, lists the pane's actions when the host
/// answers, and the row runs on that pane: Herdr gets its id, cwd, and agent,
/// not those of the pane it has focused.
#[gpui::test]
fn a_plugin_row_runs_on_the_menus_pane_not_the_focused_one(cx: &mut TestAppContext) {
    let herdr = FakeHerdr::new();
    herdr.result(
        "session.snapshot",
        json!({"snapshot": {
            "workspaces": [{"workspace_id": "w1", "label": "repo", "active_tab_id": "w1:t1"}],
            "tabs": [{"tab_id": "w1:t1", "workspace_id": "w1", "label": "main"}],
            "panes": [
                {"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1", "cwd": "/repo", "agent": "claude", "agent_status": "working"},
                {"pane_id": "inactive", "workspace_id": "w1", "tab_id": "w1:t1", "cwd": "/repo/docs", "agent": "codex", "agent_status": "idle"},
            ],
            "layouts": [{"tab_id": "w1:t1", "focused_pane_id": "w1:p1"}],
        }}),
    );
    herdr.result("plugin.action.invoke", json!({"type": "ok"}));
    let (view, cx) = window(cx);
    fake::answer(fake::hunk_and_notes());
    fake::run_through_script(herdr.path().to_owned());

    open(&view, cx, "inactive");
    // Nothing read yet: the menu's own rows show without waiting.
    assert!(rows(&view, cx).is_empty());
    draw(cx);
    assert!(cx.debug_bounds("pane-menu-0").is_some());
    cx.run_until_parked();
    assert_eq!(rows(&view, cx), ["hunk/review-pane", "notes/append"]);

    click(cx, "plugin-action-hunk/review-pane");
    assert!(view.read_with(cx, |v, _| v.menu.page.is_none()));
    cx.run_until_parked();
    let run = fake::take_invocations();
    assert_eq!(run.len(), 1);
    assert_eq!(run[0].target, PluginTarget::Pane("inactive".into()));
    assert_eq!(run[0].selection, None);
    let invoke = herdr
        .requests()
        .into_iter()
        .find(|request| request["method"] == "plugin.action.invoke")
        .unwrap();
    assert_eq!(
        invoke["params"],
        json!({
            "plugin_id": "hunk",
            "action_id": "review-pane",
            "context": {
                "workspace_id": "w1",
                "workspace_label": "repo",
                "workspace_cwd": "/repo/docs",
                "tab_id": "w1:t1",
                "tab_label": "main",
                "focused_pane_id": "inactive",
                "focused_pane_cwd": "/repo/docs",
                "focused_pane_agent": "codex",
                "focused_pane_status": "idle",
                "invocation_source": "context_menu",
            },
        })
    );
    assert!(view.read_with(cx, |v, _| v.flash.is_none()));
}

/// Each opening shows the host's last answer at once and reads it again,
/// once at a time. A failed read keeps the last answer; a plugin disabled
/// since shows until the read that the opening started lands.
#[gpui::test]
fn the_menu_shows_the_last_answer_and_reads_once_at_a_time(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    fake::answer(fake::hunk_and_notes());
    // A second opening while the first read is in flight starts none.
    open(&view, cx, "inactive");
    open(&view, cx, "inactive");
    cx.run_until_parked();
    assert_eq!(fake::fetches(), 1);
    let listed = ["hunk/review-pane", "notes/append"];
    assert_eq!(rows(&view, cx), listed);

    fake::fail_reads();
    open(&view, cx, "inactive");
    assert_eq!(rows(&view, cx), listed);
    cx.run_until_parked();
    assert_eq!(fake::fetches(), 2);
    assert_eq!(rows(&view, cx), listed);
    // A host that cannot list, as an older Herdr, says nothing.
    assert!(view.read_with(cx, |v, _| v.flash.is_none()));
    draw(cx);
    assert!(cx.debug_bounds("pane-menu-0").is_some());

    // Notes is disabled now. A read after a failure still starts.
    fake::answer(fake::catalog(
        json!([
            {"plugin_id": "hunk", "action_id": "review-pane", "title": "Review changes in Hunk", "contexts": ["pane"]},
            {"plugin_id": "notes", "action_id": "append", "title": "Append to notes", "contexts": ["pane"]},
        ]),
        json!([
            {"plugin_id": "hunk", "name": "Hunk", "enabled": true},
            {"plugin_id": "notes", "name": "Notes", "enabled": false},
        ]),
    ));
    open(&view, cx, "inactive");
    assert_eq!(rows(&view, cx), listed);
    cx.run_until_parked();
    assert_eq!(fake::fetches(), 3);
    assert_eq!(rows(&view, cx), ["hunk/review-pane"]);
    draw(cx);
    assert!(cx.debug_bounds("plugin-action-notes/append").is_none());
}

/// Down and Up walk the menu's own rows and then its plugin rows as one
/// list, wrapping at both ends; hovering selects what the keys would; Enter
/// runs the selected plugin row's action. The focused pane's menu has fewer
/// rows of its own (no Swap), and its plugin rows still map right.
#[gpui::test]
fn keys_and_hover_walk_built_in_then_plugin_rows(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    fake::answer(fake::hunk_and_notes());
    let mut own_counts = Vec::new();
    for pane in ["inactive", "w1:p1"] {
        open(&view, cx, pane);
        cx.run_until_parked();
        let (own, _) = selection(&view, cx);
        own_counts.push(own);

        cx.simulate_keystrokes("up");
        assert_eq!(selection(&view, cx).1, Some(own + 1), "{pane}");
        cx.simulate_keystrokes("enter");
        assert_eq!(fake::take_run(), ["notes/append"], "{pane}");

        open(&view, cx, pane);
        for _ in 0..own {
            cx.simulate_keystrokes("down");
        }
        assert_eq!(selection(&view, cx).1, Some(own - 1), "{pane}");
        cx.simulate_keystrokes("down");
        assert_eq!(selection(&view, cx).1, Some(own), "{pane}");
        cx.simulate_keystrokes("up");
        assert_eq!(selection(&view, cx).1, Some(own - 1), "{pane}");
        cx.simulate_keystrokes("down down down");
        assert_eq!(selection(&view, cx).1, Some(0), "{pane}: wraps");
        cx.simulate_keystrokes("up up");
        cx.simulate_keystrokes("enter");
        assert_eq!(fake::take_run(), ["hunk/review-pane"], "{pane}");

        open(&view, cx, pane);
        draw(cx);
        let row = cx.debug_bounds("plugin-action-notes/append").unwrap();
        cx.simulate_mouse_move(row.center(), None, Modifiers::default());
        assert_eq!(selection(&view, cx).1, Some(own + 1), "{pane}");
        cx.simulate_keystrokes("enter");
        assert_eq!(fake::take_run(), ["notes/append"], "{pane}");
    }
    assert_ne!(
        own_counts[0], own_counts[1],
        "both menus have the same rows"
    );
}

/// An answer that lands while the menu is open and changes its rows drops
/// a selected plugin row, which may now be another action; a selected row
/// of the menu's own, the move picker's selection, and a selection under an
/// unchanged answer stay.
#[gpui::test]
fn a_late_answer_clears_only_a_selected_plugin_row(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |v, _| {
        let snapshot = Arc::make_mut(v.live.snapshot.as_mut().unwrap());
        // More destinations than the menu has rows of its own, so the
        // picker's selection reaches indices its plugin rows also use.
        for number in 2..=20 {
            let mut tab = snapshot.tabs[0].clone();
            tab.tab_id = format!("w1:t{number}");
            tab.number = number;
            tab.label = format!("tab {number}");
            snapshot.tabs.push(tab);
        }
    });
    fake::answer(fake::hunk_and_notes());
    open(&view, cx, "inactive");
    cx.run_until_parked();
    let (own, _) = selection(&view, cx);

    // The same answer again keeps the selection.
    open(&view, cx, "inactive");
    press(cx, "up");
    cx.run_until_parked();
    assert_eq!(selection(&view, cx).1, Some(own + 1));

    fake::answer(with_aider("Ask aider"));
    open(&view, cx, "inactive");
    press(cx, "up");
    assert_eq!(selection(&view, cx).1, Some(own + 1));
    cx.run_until_parked();
    assert_eq!(rows(&view, cx)[0], "aider/ask");
    assert_eq!(selection(&view, cx).1, None);

    fake::answer(with_aider("Ask aider about this pane"));
    open(&view, cx, "inactive");
    press(cx, "down");
    cx.run_until_parked();
    assert_eq!(selection(&view, cx).1, Some(0));

    fake::answer(with_aider("Ask aider again"));
    open(&view, cx, "inactive");
    cx.update(|window, cx| {
        view.update(cx, |v, cx| v.activate_pane_menu(Action::MoveTo, window, cx))
    });
    assert_eq!(view.read_with(cx, |v, _| v.menu.page), Some(Page::MovePane));
    press(cx, "up");
    let picked = selection(&view, cx).1;
    assert!(picked.is_some_and(|index| index >= own), "{picked:?}");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.menu.page), Some(Page::MovePane));
    assert_eq!(selection(&view, cx).1, picked);
}

#[derive(Debug, Clone, Copy)]
enum Change {
    PaneClosed,
    DaemonRestarted,
    Reconnected,
    EndpointSwitched,
    Moving,
}

/// A plugin row in a menu opened before its pane closed, the daemon
/// restarted, or the connection changed refuses as the menu's own rows do
/// and sends nothing; while the pane is moving, it does nothing.
#[gpui::test]
fn a_plugin_row_refuses_once_its_pane_or_connection_changed(cx: &mut TestAppContext) {
    for change in [
        Change::PaneClosed,
        Change::DaemonRestarted,
        Change::Reconnected,
        Change::EndpointSwitched,
        Change::Moving,
    ] {
        let (view, cx) = window(cx);
        fake::answer(fake::hunk_and_notes());
        open(&view, cx, "inactive");
        cx.run_until_parked();
        cx.simulate_keystrokes("up");
        view.update(cx, |v, _| match change {
            Change::PaneClosed => Arc::make_mut(v.live.snapshot.as_mut().unwrap())
                .panes
                .truncate(1),
            Change::DaemonRestarted => Arc::make_mut(v.live.snapshot.as_mut().unwrap())
                .boot_id
                .push_str("-restarted"),
            Change::Reconnected => v.endpoints[0].generation += 1,
            Change::EndpointSwitched => v.selection_epoch += 1,
            Change::Moving => v.menu.pane.as_mut().unwrap().moving = Some(7),
        });
        cx.simulate_keystrokes("enter");
        assert!(fake::take_run().is_empty(), "{change:?}");
        view.read_with(cx, |v, _| {
            assert_eq!(v.menu.page, Some(Page::Pane), "{change:?}");
            let error = &v.menu.pane.as_ref().unwrap().error;
            assert_eq!(
                error.is_some(),
                !matches!(change, Change::Moving),
                "{change:?}: {error:?}"
            );
        });
    }
}

fn english(text: &str) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

/// Herdr's later refusal of an action, or a target gone by the time the
/// action reads Herdr's snapshot, flashes a warning naming the action and
/// saying why.
#[gpui::test]
fn a_failed_action_flashes_its_title_and_the_reason(cx: &mut TestAppContext) {
    let herdr = FakeHerdr::new();
    // Herdr no longer has the menu's pane.
    herdr.result(
        "session.snapshot",
        json!({"snapshot": {
            "workspaces": [{"workspace_id": "w1", "active_tab_id": "w1:t1"}],
            "tabs": [{"tab_id": "w1:t1", "workspace_id": "w1"}],
            "panes": [{"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1"}],
        }}),
    );
    let cases = [
        (
            Some(("plugin_disabled", "plugin hunk is disabled")),
            "Review changes in Hunk: Herdr refused the plugin action: plugin hunk is disabled",
        ),
        (
            Some(("action_not_found", "action review-pane not found")),
            "Review changes in Hunk: Herdr refused the plugin action: action review-pane not found",
        ),
        (
            None,
            "Review changes in Hunk: The plugin action's pane, tab, or workspace no longer exists.",
        ),
    ];
    for (refusal, expected) in cases {
        let (view, cx) = window(cx);
        fake::answer(fake::hunk_and_notes());
        match refusal {
            Some((code, message)) => fake::refuse(code, message),
            None => fake::run_through_script(herdr.path().to_owned()),
        }
        open(&view, cx, "inactive");
        cx.run_until_parked();
        click(cx, "plugin-action-hunk/review-pane");
        cx.run_until_parked();
        assert_eq!(fake::take_run(), ["hunk/review-pane"]);
        view.read_with(cx, |v, _| {
            let (flash, _) = v
                .flash
                .as_ref()
                .unwrap_or_else(|| panic!("no flash: {expected}"));
            assert_eq!(*flash, crate::window::Flash::warning(flash.text.clone()));
            assert_eq!(english(&flash.text), expected);
        });
    }
    assert!(
        !herdr
            .requests()
            .iter()
            .any(|request| request["method"] == "plugin.action.invoke")
    );
}

/// Many plugins under a short window: the row the keys select scrolls into
/// view, though every plugin's heading sits between the rows.
#[gpui::test]
fn keyboard_selection_scrolls_plugin_rows_into_view(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(800.), px(320.)));
    let plugins = ["aider", "beads", "hunk", "notes", "review", "tasks"];
    let actions: Vec<_> = plugins
        .iter()
        .flat_map(|plugin| {
            ["first", "second"].map(|action| {
                json!({"plugin_id": plugin, "action_id": action, "title": format!("{plugin} {action}"), "contexts": ["pane"]})
            })
        })
        .collect();
    let installed: Vec<_> = plugins
        .iter()
        .map(|plugin| json!({"plugin_id": plugin, "name": plugin, "enabled": true}))
        .collect();
    fake::answer(fake::catalog(json!(actions), json!(installed)));
    open(&view, cx, "inactive");
    cx.run_until_parked();
    draw(cx);
    let panel = cx.debug_bounds("menu-panel").unwrap();
    let last = cx.debug_bounds("plugin-action-tasks/second").unwrap();
    assert!(last.bottom() > panel.bottom(), "the menu fits: {panel:?}");

    let inside = |cx: &mut VisualTestContext, selector: &'static str| {
        draw(cx);
        let panel = cx.debug_bounds("menu-panel").unwrap();
        let row = cx.debug_bounds(selector).unwrap();
        assert!(
            row.top() >= panel.top() && row.bottom() <= panel.bottom(),
            "{selector}: {row:?} outside {panel:?}"
        );
    };
    cx.simulate_keystrokes("up");
    inside(cx, "plugin-action-tasks/second");
    cx.simulate_keystrokes("up up up up");
    inside(cx, "plugin-action-notes/second");
    cx.simulate_keystrokes("down down down down down");
    inside(cx, "pane-menu-0");
}

/// A long title or plugin name ends with "…" rather than widening the menu.
#[gpui::test]
fn long_plugin_titles_do_not_widen_the_menu(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let title = "Review every changed file in this pane's working tree with Hunk, \
                 including untracked files, submodules, and generated lockfiles";
    let name = "Hunk: interactive review for agent-written changes, nightly channel build";
    fake::answer(fake::catalog(
        json!([{"plugin_id": "hunk", "action_id": "review", "title": title, "contexts": ["pane"]}]),
        json!([{"plugin_id": "hunk", "name": name, "enabled": true}]),
    ));
    open(&view, cx, "inactive");
    cx.run_until_parked();
    draw(cx);
    let cap = px(320. * view.read_with(cx, |v, _| v.config.ui.size) / 12.);
    let label = cx.debug_bounds("plugin-action-label-hunk/review").unwrap();
    assert!(label.size.width <= cap, "{label:?}");
    assert!(cx.debug_bounds("plugin-heading-hunk").is_some());
    let panel = cx.debug_bounds("menu-panel").unwrap();
    // Beyond the cap: rows' padding, the panel's own, and its border. The
    // whole title would be more than twice the cap.
    assert!(
        panel.size.width <= cap + px(48.),
        "{panel:?} wider than {cap:?}"
    );
}
