//! The header's Clear all, which runs Clear All Notifications from the top
//! of the page.

use super::{
    alert_shelf::alert,
    page::{bounds, click, draw, fixture},
};
use sheprd_client::protocol::SemanticNotificationKind as Kind;

#[gpui::test]
fn nothing_to_clear_offers_no_clear_all(cx: &mut gpui::TestAppContext) {
    let (_, cx) = fixture(cx);
    assert!(cx.debug_bounds("overview-summary").is_some());
    assert!(cx.debug_bounds("overview-clear-all").is_none());
}

#[gpui::test]
fn an_alert_offers_clear_all_in_the_header(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let before = bounds(cx, "overview-summary").unwrap();
    alert(&view, cx);
    let summary = bounds(cx, "overview-summary").unwrap();
    let clear = bounds(cx, "overview-clear-all").unwrap();
    assert_eq!(summary, before, "the header keeps its place and height");
    assert!(summary.contains(&clear.center()));
    // At the far end of the header, and above the shelf at the page's foot.
    assert!(clear.right() > summary.center().x);
    let shelf = bounds(cx, "overview-alerts-clear").unwrap();
    assert!(clear.bottom() <= shelf.top());
}

#[gpui::test]
fn clear_all_empties_every_host_and_goes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    alert(&view, cx);
    click(cx, "overview-clear-all");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.overview_alerts().is_empty());
        assert!(view.alert_infos().is_empty());
        assert!(!view.overview.alerts_open);
        assert!(view.endpoints.iter().all(|e| e.toasts.entries.is_empty()));
    });
    assert!(cx.debug_bounds("overview-alerts").is_none());
    assert!(cx.debug_bounds("overview-clear-all").is_none());
}

#[gpui::test]
fn a_toast_on_screen_offers_clear_all(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    // A custom notice shows as a toast but is never kept as an alert.
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_toast_preview(Kind::Custom, cx)));
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.overview_alerts().is_empty());
        let mut shown = view.endpoints.iter().flat_map(|e| &e.toasts.entries);
        assert!(shown.any(|(_, notice)| notice.visible));
    });
    assert!(cx.debug_bounds("overview-alerts").is_none());
    click(cx, "overview-clear-all");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.endpoints.iter().all(|e| e.toasts.entries.is_empty()));
    });
    assert!(cx.debug_bounds("overview-clear-all").is_none());
}
