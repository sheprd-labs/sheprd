//! Plugin actions when the user picks another session behind one device.
//!
//! An endpoint keeps its id when its session changes; only its connection
//! target does. What the tab and workspace menus list and run must follow the
//! session, not the endpoint: one session's actions never show in another's
//! menus, a read for a session the window left never changes the menu it is
//! on, and the session's last answer shows at once when the window returns.
//! The pane menu has its own test beside its other plugin tests.

use super::fake;
use crate::{SheprdWindow, plugin_actions::catalog::Catalog};
use gpui::{Entity, TestAppContext, VisualTestContext, point, px, size};
use sheprd_client::ConnectTarget;

#[derive(Debug, Clone, Copy)]
enum Menu {
    Tab,
    Workspace,
}

const MENUS: [Menu; 2] = [Menu::Tab, Menu::Workspace];

impl Menu {
    /// The element names of the rows [`fake::hunk_and_notes`] gives this menu.
    fn hunk_and_notes(self) -> Vec<&'static str> {
        match self {
            Self::Tab => vec![
                "plugin-action-hunk/review-tab",
                "plugin-action-notes/tab-summary",
            ],
            Self::Workspace => vec![
                "plugin-action-hunk/review-workspace",
                "plugin-action-notes/workspace-log",
            ],
        }
    }

    /// The element names of the rows [`aider`] gives this menu.
    fn aider(self) -> Vec<&'static str> {
        match self {
            Self::Tab => vec!["plugin-action-aider/ask-tab"],
            Self::Workspace => vec!["plugin-action-aider/ask-workspace"],
        }
    }
}

/// One session's actions, the other's being [`fake::hunk_and_notes`].
fn aider() -> Catalog {
    fake::catalog(
        serde_json::json!([
            {"plugin_id": "aider", "action_id": "ask-tab", "title": "Ask aider about this tab", "contexts": ["tab"]},
            {"plugin_id": "aider", "action_id": "ask-workspace", "title": "Ask aider about this workspace", "contexts": ["workspace"]},
        ]),
        serde_json::json!([{"plugin_id": "aider", "name": "Aider", "enabled": true}]),
    )
}

/// Two sessions of one saved device: the same endpoint, another target.
fn session(name: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: "dev@box".into(),
        session: name.into(),
    }
}

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        fake::install(&mut view);
        view
    });
    cx.simulate_resize(size(px(1000.), px(900.)));
    (view, cx)
}

/// What the app does when the user picks another session of the selected
/// device (`select_device_session`), without its reconnect: the endpoint keeps
/// its id and takes the session's target.
fn switch_to(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, target: &ConnectTarget) {
    view.update(cx, |v, _| {
        v.endpoints[v.selected_endpoint].connection.target = target.clone();
    });
}

/// Opens `menu` and leaves the host's answer unread.
fn open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, menu: Menu) {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            match menu {
                Menu::Tab => v.open_tab_menu("t1", None, point(px(100.), px(40.)), window, cx),
                Menu::Workspace => v.open_workspace_menu("w3", Default::default(), window, cx),
            }
            assert!(v.menu.page.is_some(), "{menu:?} did not open");
        })
    });
}

/// The plugin rows the open menu shows, by element name.
fn shown(cx: &mut VisualTestContext, menu: Menu) -> Vec<&'static str> {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    menu.hunk_and_notes()
        .into_iter()
        .chain(menu.aider())
        .filter(|name| cx.debug_bounds(name).is_some())
        .collect()
}

/// A menu opened after the window switched session lists that session's
/// actions, never the ones the other session answered, and the other session's
/// answer is still there, at once, when the window switches back.
#[gpui::test]
fn a_menu_lists_the_actions_of_the_session_it_is_on(cx: &mut TestAppContext) {
    let (default, review) = (session("default"), session("review"));
    for menu in MENUS {
        let (view, cx) = window(cx);
        fake::answer_on(&default, fake::hunk_and_notes());
        fake::answer_on(&review, aider());

        switch_to(&view, cx, &default);
        open(&view, cx, menu);
        cx.run_until_parked();
        assert_eq!(shown(cx, menu), menu.hunk_and_notes(), "{menu:?}: default");

        switch_to(&view, cx, &review);
        open(&view, cx, menu);
        assert_eq!(
            shown(cx, menu),
            Vec::<&str>::new(),
            "{menu:?}: review, before it answers, shows default's actions"
        );
        cx.run_until_parked();
        assert_eq!(shown(cx, menu), menu.aider(), "{menu:?}: review");

        // Back on the first session its last answer is there before any read.
        switch_to(&view, cx, &default);
        open(&view, cx, menu);
        assert_eq!(
            shown(cx, menu),
            menu.hunk_and_notes(),
            "{menu:?}: default again, before it is read again"
        );
    }
}

/// A read started for the session the window then left lands while the menu
/// is open on the new session: it changes neither that menu nor what the new
/// session answers, even when the new session has answered nothing yet.
#[gpui::test]
fn a_late_answer_for_the_session_left_does_not_change_the_open_menu(cx: &mut TestAppContext) {
    let (default, review) = (session("default"), session("review"));
    for menu in MENUS {
        let (view, cx) = window(cx);
        fake::answer_on(&default, fake::hunk_and_notes());
        // The review session cannot list its actions, as a host without Herdr.

        switch_to(&view, cx, &default);
        open(&view, cx, menu);
        // The read for `default` is in flight: nothing has run it yet.
        switch_to(&view, cx, &review);
        open(&view, cx, menu);
        cx.run_until_parked();

        assert_eq!(
            shown(cx, menu),
            Vec::<&str>::new(),
            "{menu:?}: default's late answer shows in review's open menu"
        );
        open(&view, cx, menu);
        assert_eq!(
            shown(cx, menu),
            Vec::<&str>::new(),
            "{menu:?}: default's late answer is kept as review's"
        );
    }
}

/// A read still in flight for one session does not stop a menu opened on
/// another from reading its own.
#[gpui::test]
fn a_read_in_flight_for_another_session_does_not_stop_this_sessions_read(cx: &mut TestAppContext) {
    let (default, review) = (session("default"), session("review"));
    for menu in MENUS {
        let (view, cx) = window(cx);
        fake::answer_on(&default, fake::hunk_and_notes());
        fake::answer_on(&review, aider());

        switch_to(&view, cx, &default);
        open(&view, cx, menu);
        switch_to(&view, cx, &review);
        open(&view, cx, menu);
        cx.run_until_parked();

        let mut fetched = fake::take_fetched_hosts();
        fetched.sort();
        let mut both = vec![fake::host_of(&default), fake::host_of(&review)];
        both.sort();
        assert_eq!(fetched, both, "{menu:?}: each session is read once");
        assert_eq!(shown(cx, menu), menu.aider(), "{menu:?}");
    }
}

/// A row runs against the host of the session the window is on when it is
/// chosen: each session's host, not the first one a menu was opened on.
#[gpui::test]
fn a_plugin_row_runs_on_the_host_of_the_session_the_menu_is_on(cx: &mut TestAppContext) {
    let (default, review) = (session("default"), session("review"));
    let (view, cx) = window(cx);
    fake::answer_on(&default, fake::hunk_and_notes());
    fake::answer_on(&review, aider());

    for (target, row, run) in [
        (&default, "plugin-action-hunk/review-tab", "hunk/review-tab"),
        (&review, "plugin-action-aider/ask-tab", "aider/ask-tab"),
    ] {
        switch_to(&view, cx, target);
        open(&view, cx, Menu::Tab);
        cx.run_until_parked();
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let bounds = cx.debug_bounds(row).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(fake::take_run(), [run]);
        assert_eq!(fake::take_run_hosts(), [fake::host_of(target)]);
    }
}
