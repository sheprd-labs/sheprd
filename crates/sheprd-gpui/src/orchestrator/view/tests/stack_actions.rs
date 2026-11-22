//! The stack popover's footer merges the stack once every open layer up to
//! this one can merge, and adds it to or takes it out of the merge queue,
//! each through the view's confirmation naming what lands.
use super::stack::open_stacked;
use crate::{
    orchestrator::view::{dispatch::Confirm, stack::Change},
    pull_request::{MergeMethod, QueueEntry, QueueState, ready_stacked_fixture, stacked_fixture},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

fn open_popover(cx: &mut VisualTestContext) {
    let chip = cx.debug_bounds("orchestrator-stack-chip").unwrap();
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_ready_stack_merges_with_its_only_method_after_naming_what_lands(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.merge_methods = vec![MergeMethod::Squash];
    let (_dir, view, cx) = open_stacked(cx, pr);
    open_popover(cx);
    click(cx, "orchestrator-stack-merge");
    cx.update(|_, cx| {
        let view = view.read(cx);
        let Some(Confirm::Land {
            change,
            target,
            lands,
        }) = &view.confirm
        else {
            panic!("expected a stack confirmation, got {:?}", view.confirm);
        };
        assert_eq!(*change, Change::Merge(MergeMethod::Squash));
        assert_eq!(target.number, 1364);
        assert_eq!(lands, &[1362, 1364]);
        assert_eq!(view.stack.open, None);
        let (title, body, ..) = super::super::stack::confirm_text(*change, target, lands);
        assert_eq!(title, "Merge \u{2068}2\u{2069} pull requests?");
        assert!(body.contains("#1362, #1364"), "{body}");
    });
}

#[gpui::test]
fn with_several_methods_merging_a_stack_asks_which_first(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.merge_methods = vec![MergeMethod::Squash, MergeMethod::Rebase];
    let (_dir, view, cx) = open_stacked(cx, pr);
    open_popover(cx);
    click(cx, "orchestrator-stack-merge");
    cx.update(|_, cx| assert!(view.read(cx).confirm.is_none()));
    let squash = cx.debug_bounds("orchestrator-stack-merge-SQUASH");
    let popover = cx.debug_bounds("orchestrator-stack-popover").unwrap();
    assert!(squash.is_some_and(|squash| popover.contains(&squash.center())));
}

#[gpui::test]
fn a_blocked_stack_offers_no_merge_but_can_still_be_queued(cx: &mut TestAppContext) {
    let mut pr = stacked_fixture().unwrap();
    pr.merge_queue = true;
    let (_dir, view, cx) = open_stacked(cx, pr);
    open_popover(cx);
    assert!(cx.debug_bounds("orchestrator-stack-merge").is_none());
    click(cx, "orchestrator-stack-enqueue");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(matches!(
            &view.confirm,
            Some(Confirm::Land { change: Change::Enqueue, lands, .. }) if lands == &[1362, 1364]
        ));
    });
}

#[gpui::test]
fn a_queued_stack_can_only_be_taken_out_of_the_queue(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.queue = Some(QueueEntry {
        state: QueueState::Queued,
        position: 1,
        eta: None,
    });
    let (_dir, view, cx) = open_stacked(cx, pr);
    open_popover(cx);
    assert!(cx.debug_bounds("orchestrator-stack-merge").is_none());
    assert!(cx.debug_bounds("orchestrator-stack-enqueue").is_none());
    let remove = cx.debug_bounds("orchestrator-stack-dequeue");
    assert!(remove.is_some());
    let at = remove.unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(matches!(
            &view.read(cx).confirm,
            Some(Confirm::Land {
                change: Change::Dequeue,
                ..
            })
        ));
    });
}

#[gpui::test]
fn a_draft_stack_offers_nothing_to_merge_or_queue(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.is_draft = true;
    let (_dir, _view, cx) = open_stacked(cx, pr);
    open_popover(cx);
    assert!(cx.debug_bounds("orchestrator-stack-merge").is_none());
    assert!(cx.debug_bounds("orchestrator-stack-enqueue").is_none());
}

#[gpui::test]
fn a_pull_request_in_no_stack_is_queued_from_its_status_box(cx: &mut TestAppContext) {
    let mut pr = crate::pull_request::fixture().unwrap();
    pr.number = 1364;
    pr.url = "https://github.com/example/project/pull/1364".into();
    pr.merge_queue = true;
    let (_dir, view, cx) = open_stacked(cx, pr);
    click(cx, "orchestrator-detail-queue");
    cx.update(|_, cx| {
        assert!(matches!(
            &view.read(cx).confirm,
            Some(Confirm::Land { change: Change::Enqueue, lands, .. }) if lands == &[1364]
        ));
    });
}

#[gpui::test]
fn the_status_boxs_merge_of_a_stacked_pull_request_names_every_layer(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.merge_methods = vec![MergeMethod::Squash];
    let (_dir, view, cx) = open_stacked(cx, pr);
    click(cx, "orchestrator-detail-merge");
    click(cx, "orchestrator-merge-SQUASH");
    cx.update(|_, cx| {
        assert!(matches!(
            &view.read(cx).confirm,
            Some(Confirm::Land { change: Change::Merge(MergeMethod::Squash), lands, .. })
                if lands == &[1362, 1364]
        ));
    });
}

#[gpui::test]
fn a_blocked_stack_offers_no_merge_in_the_status_box(cx: &mut TestAppContext) {
    let mut pr = stacked_fixture().unwrap();
    pr.merge_methods = vec![MergeMethod::Squash];
    let (_dir, _view, cx) = open_stacked(cx, pr);
    assert!(cx.debug_bounds("orchestrator-detail-merge").is_none());
}
