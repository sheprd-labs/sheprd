//! The Pull requests list's columns fit what they hold: the number column
//! grows to the longest number listed, and a list too narrow for every
//! column keeps the compact ones, as it does beside an open item, rather
//! than squeezing the title away.
use super::*;
use crate::orchestrator::{PullRequest, view::rows::Tab};
use gpui::{VisualTestContext, px, size};
use std::sync::Arc;

const TITLE: &str = "orchestrator-row-title";

fn pull_request(identifier: &str) -> Item {
    let mut pr = item(Provider::Github, "pr/8", "Menus", "2026-10-04T00:00:00Z");
    pr.identifier = identifier.into();
    pr.pull_request = Some(PullRequest {
        number: 8,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: "feature".into(),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pr
}

/// Lists `listed` alone on the Pull requests tab, nothing open, in a window
/// `width` wide, once the list has measured itself.
fn list(
    cx: &mut gpui::TestAppContext,
    listed: Item,
    width: f32,
) -> (tempfile::TempDir, String, &mut VisualTestContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.simulate_resize(size(px(width), px(600.)));
    let key = listed.key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(vec![listed]);
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    // Columns follow the width the previous frame measured.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (dir, key, cx)
}

fn review_shown(cx: &mut VisualTestContext, key: &str) -> bool {
    let selector: &'static str = Box::leak(format!("orchestrator-review-{key}").into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn a_long_number_widens_its_column(cx: &mut gpui::TestAppContext) {
    let (_dir, _, cx) = list(cx, pull_request("!1234567"), 1400.);
    let list = cx.debug_bounds("orchestrator-list").unwrap();
    let title = cx.debug_bounds(TITLE).unwrap();
    // Row padding, the icon and its gap, eight characters of at least half
    // the usual 12 px small text, and the gap before the title: wider than
    // the column's usual 60 px.
    let number = title.left() - list.left() - px(16. + 16.);
    assert!(number >= px(16. + 8. * 6.), "{number:?}");
    assert!(number > px(60.), "{number:?}");
}

#[gpui::test]
fn a_wide_list_shows_every_column(cx: &mut gpui::TestAppContext) {
    let (_dir, key, cx) = list(cx, pull_request("#1406"), 1400.);
    assert!(review_shown(cx, &key));
}

#[gpui::test]
fn a_narrow_list_keeps_its_title_over_the_status_columns(cx: &mut gpui::TestAppContext) {
    let (_dir, key, cx) = list(cx, pull_request("#1406"), 740.);
    assert!(!review_shown(cx, &key));
    let title = cx.debug_bounds(TITLE).unwrap();
    assert!(title.size.width >= px(200.), "{title:?}");
}
