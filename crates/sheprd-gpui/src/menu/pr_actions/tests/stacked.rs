//! A stacked pull request merges with the open layers below it, so the Git
//! popup offers Merge only once they are all known and ready, and its dialog
//! names them.
use super::{draw, labels, open};
use crate::{
    menu::Page,
    pull_request::{MergeMethod, ready_stacked_fixture, stacked_fixture},
};
use gpui::TestAppContext;

#[gpui::test]
fn a_blocked_stack_offers_no_merge(cx: &mut TestAppContext) {
    let mut pr = stacked_fixture().unwrap();
    pr.merge_methods = vec![MergeMethod::Squash];
    let (view, cx) = open(cx, pr);
    cx.update(|_, cx| {
        assert!(
            !labels(view.read(cx))
                .iter()
                .any(|label| label == "Merge...")
        );
    });
}

#[gpui::test]
fn a_ready_stacks_merge_dialog_names_what_it_lands(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.merge_methods = vec![MergeMethod::Squash];
    let (view, cx) = open(cx, pr);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(labels(view).iter().any(|label| label == "Merge..."));
            view.open_pr_merge();
            assert_eq!(view.menu.page, Some(Page::PrMerge));
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("pr-merge-stack").is_some());
}
