use super::*;
use crate::{
    sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw},
    traces::{
        Detail,
        service::{Snapshot, Status},
    },
};
use gpui::{AppContext as _, px, size};
use sheprd_control_protocol::ActivityPanel;
use std::sync::Arc;

#[gpui::test]
fn narrow_tables_scroll_inside_the_page_with_and_without_details(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.activity.panel = ActivityPanel::Traces;
            let mut session = session("long-title", AgentKind::Claude, 0, 12);
            session.updated_at = Some(chrono::Utc::now().timestamp_millis());
            session.title =
                Some("A long session title that must not push columns outside the window".into());
            view.traces.status = Status::Ready(Arc::new(Snapshot {
                sessions: vec![session],
                activity: Vec::new(),
                ..Snapshot::default()
            }));
            view.poll_traces(cx);
            let point = sheprd_traces::OverlapPoint { at: NOW, count: 3 };
            view.traces
                .page
                .as_mut()
                .unwrap()
                .overlap
                .fixture(sheprd_traces::Overlap {
                    recent: point,
                    recent_by_agent: vec![],
                    daily: vec![point],
                });
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    for detail in [false, true] {
        view.update(cx, |view, cx| {
            let page = view.traces.page.as_mut().unwrap();
            page.selected = detail.then(|| "long-title".into());
            page.detail = detail.then(|| Detail::Loaded(Vec::new()));
            cx.notify();
        });
        for width in [360., 640., 1400.] {
            cx.simulate_resize(size(px(width), px(1200.)));
            cx.update(|window, cx| full_draw(window, cx).clear(cx));
            let page = cx.debug_bounds("traces-section").unwrap();
            let viewport = cx.debug_bounds("traces-table-viewport").unwrap();
            let table = cx.debug_bounds("traces-table").unwrap();
            assert!(viewport.left() >= page.left(), "{viewport:?} in {page:?}");
            assert!(viewport.right() <= page.right(), "{viewport:?} in {page:?}");
            assert!(table.size.width >= px(740.), "{table:?}");
            if width < 740. {
                assert!(table.size.width > viewport.size.width);
            }
        }
    }

    let peak = cx.debug_bounds("traces-overlap-day-20774").unwrap();
    cx.simulate_click(peak.center(), gpui::Modifiers::default());
    assert!(view.read_with(cx, |v, _| {
        v.traces.page.as_ref().unwrap().overlap.selection.is_some()
    }));
}
