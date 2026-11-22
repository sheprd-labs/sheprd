//! Each dropdown hangs from its own control, right edges aligned where the
//! window has room, rather than from the view's right edge, and follows the
//! control when the search row wraps.
use crate::orchestrator::{
    Request, Timing,
    view::{Look, Menu, OrchestratorView, rows::Tab},
};
use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, px};

fn view(
    cx: &mut TestAppContext,
    width: f32,
) -> (gpui::Entity<OrchestratorView>, &mut VisualTestContext) {
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
    cx.simulate_resize(gpui::size(px(width), px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            view.toggle_menu(Menu::State);
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
}

/// The State menu's first row sits under its control and spans it, short
/// of its frame's padding.
fn assert_under_control(cx: &mut VisualTestContext) {
    let control = bounds(cx, "orchestrator-state");
    let row = bounds(cx, "orchestrator-state-open");
    assert!(row.top() >= control.bottom(), "{row:?} under {control:?}");
    assert!(
        row.left() <= control.left() && row.right() + px(8.) >= control.right(),
        "{row:?} spans {control:?}"
    );
}

#[gpui::test]
fn a_dropdown_hangs_from_its_control(cx: &mut TestAppContext) {
    let (_, cx) = view(cx, 1400.);
    assert_under_control(cx);
    // Right edges aligned, within the frame's padding, not pinned to the
    // view's right edge, where the menus used to open.
    let control = bounds(cx, "orchestrator-state");
    let row = bounds(cx, "orchestrator-state-open");
    assert!((row.right() - control.right()).abs() <= px(8.), "{row:?}");
    assert!(row.right() < px(1400. - 100.), "{row:?}");
}

#[gpui::test]
fn a_dropdown_follows_its_control_when_the_search_row_wraps(cx: &mut TestAppContext) {
    let (_, cx) = view(cx, 1400.);
    cx.simulate_resize(gpui::size(px(700.), px(600.)));
    cx.run_until_parked();
    // The row wraps from the list width the last frame measured.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-search-stacked").is_some());
    assert_under_control(cx);
}
