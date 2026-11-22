//! An open dropdown shuts when the pointer is pressed anywhere outside it;
//! a press on its own control leaves the shutting to the control.
use crate::orchestrator::{
    Request, Timing,
    view::{Look, Menu, OrchestratorView, rows::Tab},
};
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};

fn view(cx: &mut TestAppContext) -> (gpui::Entity<OrchestratorView>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: sheprd_client::ConnectTarget::Local,
        checkout: dir.path().join("web").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        azure_store: Default::default(),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

fn open(view: &gpui::Entity<OrchestratorView>, cx: &mut VisualTestContext) -> Option<Menu> {
    cx.update(|_, cx| view.read(cx).menu)
}

fn click(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

fn center(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Point<gpui::Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center()
}

#[gpui::test]
fn a_press_outside_shuts_the_open_dropdown(cx: &mut TestAppContext) {
    let (view, cx) = view(cx);
    let control = center(cx, "orchestrator-state");
    click(cx, control);
    assert_eq!(open(&view, cx), Some(Menu::State));
    // The menu's own padding is inside it.
    let frame = cx
        .debug_bounds("orchestrator-state-open")
        .unwrap_or_else(|| panic!("the menu is not drawn"))
        .origin;
    click(cx, frame - point(px(2.), px(2.)));
    assert_eq!(open(&view, cx), Some(Menu::State));
    // Far from the menu and its control.
    click(cx, point(px(40.), px(560.)));
    assert_eq!(open(&view, cx), None);
}

#[gpui::test]
fn a_right_press_outside_shuts_it_too(cx: &mut TestAppContext) {
    let (view, cx) = view(cx);
    let control = center(cx, "orchestrator-state");
    click(cx, control);
    cx.simulate_mouse_down(
        point(px(40.), px(560.)),
        MouseButton::Right,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(open(&view, cx), None);
}

#[gpui::test]
fn its_control_still_toggles_it(cx: &mut TestAppContext) {
    let (view, cx) = view(cx);
    let control = center(cx, "orchestrator-state");
    click(cx, control);
    assert_eq!(open(&view, cx), Some(Menu::State));
    // Not shut by the press and opened again by the click.
    click(cx, control);
    assert_eq!(open(&view, cx), None);
}

#[gpui::test]
fn another_control_switches_to_its_menu(cx: &mut TestAppContext) {
    let (view, cx) = view(cx);
    let state = center(cx, "orchestrator-state");
    click(cx, state);
    let sort = center(cx, "orchestrator-sort");
    click(cx, sort);
    assert_eq!(open(&view, cx), Some(Menu::Sort));
}
