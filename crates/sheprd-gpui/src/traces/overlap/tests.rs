use super::{Panel, Selection, selected_at};
use sheprd_traces::{Overlap, OverlapPoint};
use std::time::Instant;

#[test]
fn a_selected_peak_keeps_its_instant_when_replication_moves_the_peak() {
    let old = OverlapPoint {
        at: 10_000,
        count: 2,
    };
    let selection = Selection::Peak(old.at);
    let mut chart = Overlap {
        daily: vec![old],
        recent: old,
        recent_by_agent: Vec::new(),
    };
    assert_eq!(selected_at(&chart, selection), Some(old.at));

    chart.daily[0] = OverlapPoint {
        at: 5_000,
        count: 3,
    };
    chart.recent.at += 30_000;
    assert_eq!(selected_at(&chart, selection), Some(old.at));
    assert_eq!(
        selected_at(&chart, Selection::Recent),
        Some(chart.recent.at)
    );

    chart.daily.clear();
    assert_eq!(selected_at(&chart, selection), None);
}

#[test]
fn picking_sessions_keeps_the_chart_refresh_deadline() {
    let refreshed = Instant::now();
    let mut panel = Panel {
        refreshed: Some(refreshed),
        ..Panel::default()
    };
    panel.pick(Some(Selection::Peak(10_000)));
    assert!(panel.loading_sessions());
    assert_eq!(panel.refreshed, Some(refreshed));
}
