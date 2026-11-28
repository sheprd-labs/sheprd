//! Plugin actions in the tab menu, against a stand-in host: the menu lists
//! tab actions only, runs them on its own tab, and refuses once that tab
//! is gone.

use super::*;
use crate::plugin_actions::{Target as PluginTarget, tests::fake};

/// The fixture's session with a second tab, `w1:t2`, which is not focused.
fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let mut tab = snapshot.tabs[0].clone();
        tab.tab_id = "w1:t2".into();
        tab.number = 2;
        tab.label = "tests".into();
        tab.focused = false;
        snapshot.tabs.push(tab);
        view.live.snapshot = Some(Arc::new(snapshot));
        fake::install(&mut view);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

fn open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_tab_menu("w1:t2", None, point(px(100.), px(40.)), window, cx);
            assert_eq!(v.menu.page, Some(Page::Tab));
        })
    });
    cx.run_until_parked();
}

fn rows(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| {
        v.menu
            .tab
            .as_ref()
            .unwrap()
            .plugins
            .iter()
            .map(|row| format!("{}/{}", row.plugin_id, row.action_id))
            .collect()
    })
}

/// The tab menu lists the tab actions and runs a clicked one, or the one
/// Enter picks after the menu's own rows, on the menu's tab.
#[gpui::test]
fn a_tab_plugin_row_runs_on_the_menus_tab(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    fake::answer(fake::hunk_and_notes());
    open(&view, cx);
    assert_eq!(rows(&view, cx), ["hunk/review-tab", "notes/tab-summary"]);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds("plugin-action-hunk/review-tab").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    cx.run_until_parked();
    let run = fake::take_invocations();
    assert_eq!(run.len(), 1);
    assert_eq!(run[0].action_id, "review-tab");
    assert_eq!(run[0].target, PluginTarget::Tab("w1:t2".into()));
    assert!(view.read_with(cx, |v, _| v.menu.page.is_none()));

    // Away from the rows, so hovering selects none.
    cx.simulate_mouse_move(point(px(790.), px(590.)), None, Modifiers::default());
    open(&view, cx);
    let own = view.read_with(cx, |v, _| v.menu.tab.as_ref().unwrap().actions().len());
    for _ in 0..=own {
        cx.simulate_keystrokes("down");
    }
    let selected = view.read_with(cx, |v, _| v.menu.tab.as_ref().unwrap().selected);
    assert_eq!(selected, Some(own));
    cx.simulate_keystrokes("down enter");
    let run = fake::take_invocations();
    assert_eq!(run.len(), 1);
    assert_eq!(run[0].action_id, "tab-summary");
    assert_eq!(run[0].target, PluginTarget::Tab("w1:t2".into()));
}

/// A plugin row in a menu opened before its tab closed or the connection
/// changed refuses, as the menu's own rows do, and sends nothing.
#[gpui::test]
fn a_tab_plugin_row_refuses_once_its_tab_is_gone(cx: &mut TestAppContext) {
    for reconnect in [false, true] {
        let (view, cx) = window(cx);
        fake::answer(fake::hunk_and_notes());
        open(&view, cx);
        cx.simulate_keystrokes("up");
        view.update(cx, |v, _| {
            if reconnect {
                v.endpoints[0].generation += 1;
            } else {
                Arc::make_mut(v.live.snapshot.as_mut().unwrap())
                    .tabs
                    .truncate(1);
            }
        });
        cx.simulate_keystrokes("enter");
        assert!(fake::take_run().is_empty(), "reconnect {reconnect}");
        view.read_with(cx, |v, _| {
            assert_eq!(v.menu.page, Some(Page::Tab));
            assert!(v.menu.tab.as_ref().unwrap().error.is_some());
        });
    }
}
