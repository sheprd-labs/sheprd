//! `sheprd pr merge` and `pr queue` open the Git popup's confirmations, which
//! name what lands; a stacked merge says it lands the layers below.
use super::{draw, open};
use crate::{
    menu::{Page, PrChange},
    pull_request::{QueueEntry, QueueState, ready_stacked_fixture},
};
use gpui::{TestAppContext, VisualTestContext};
use sheprd_control_protocol::protocol::hosts::QueueOp;

fn ask(
    view: &gpui::Entity<crate::SheprdWindow>,
    cx: &mut VisualTestContext,
    change: PrChange,
) -> bool {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            view.ask_pr_change(change, window, cx)
        })
    })
}

#[gpui::test]
fn queueing_a_stack_opens_a_confirmation_naming_every_layer_that_lands(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, ready_stacked_fixture().unwrap());
    assert!(ask(&view, cx, PrChange::Queue(QueueOp::Add)));
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::PrQueue)));
    assert!(cx.debug_bounds("pr-queue-body").is_some());
    assert!(cx.debug_bounds("pr-queue-confirm").is_some());
    let lands = cx.update(|_, cx| view.read(cx).git_open_pull_request().unwrap().lands());
    assert_eq!(lands, [1362, 1364]);
}

#[gpui::test]
fn a_stacked_merge_dialog_says_it_lands_the_layers_below(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.merge_methods = vec![crate::pull_request::MergeMethod::Squash];
    let (view, cx) = open(cx, pr);
    assert!(ask(&view, cx, PrChange::Merge));
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::PrMerge)));
    assert!(cx.debug_bounds("pr-merge-stack").is_some());
}

#[gpui::test]
fn a_command_never_replaces_an_open_menu(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.queue = Some(QueueEntry {
        state: QueueState::Queued,
        position: 1,
        eta: None,
    });
    let (view, cx) = open(cx, pr);
    let opened = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ask_pr_change(PrChange::Queue(QueueOp::Remove), window, cx)
        })
    });
    assert!(!opened, "the Git popup the test opened stays");
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::Git)));
}

#[gpui::test]
fn enter_never_confirms_a_dialog_a_command_opened(cx: &mut TestAppContext) {
    let mut pr = ready_stacked_fixture().unwrap();
    pr.merge_methods = vec![crate::pull_request::MergeMethod::Squash];
    let (view, cx) = open(cx, pr);
    for (change, page) in [
        (PrChange::Queue(QueueOp::Add), Page::PrQueue),
        (PrChange::Merge, Page::PrMerge),
    ] {
        assert!(ask(&view, cx, change));
        draw(cx);
        // An Enter typed for a terminal, landing on the dialog.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.menu.page, Some(page));
            assert!(view.pr_actions.running().is_none(), "nothing was sent");
        });
    }
}
