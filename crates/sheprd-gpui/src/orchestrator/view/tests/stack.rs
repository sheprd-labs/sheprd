//! A stacked pull request's chip opens a popover that says what merging
//! there would do and lists the layers, each opening its own page.
use super::*;
use crate::orchestrator::{
    PullRequest,
    service::PullRequestLookup,
    view::{OrchestratorView, rows::Tab},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px};
use std::sync::Arc;

pub(super) fn listed(number: u64) -> Item {
    let mut pr = item(
        Provider::Github,
        &format!("pr/{number}"),
        "T3",
        "2026-11-17T00:00:00Z",
    );
    pr.pull_request = Some(PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("feat/{number}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pr
}

/// Opens #1364's page, looked up as layer 3 of 4, with #1362 listed too.
pub(super) fn open_stacked(
    cx: &mut TestAppContext,
    looked_up: crate::pull_request::PullRequest,
) -> (
    tempfile::TempDir,
    gpui::Entity<OrchestratorView>,
    &mut VisualTestContext,
) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let current = listed(1364);
            let key = current.key.canonical();
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(vec![current, listed(1362)]);
            repo.snapshot.pull_request = Some(PullRequestLookup {
                number: 1364,
                result: Arc::new(Ok(Some(looked_up))),
            });
            view.show_list(Tab::PullRequests, cx);
            view.open_detail(key, cx);
        })
    });
    cx.run_until_parked();
    // The page settles on its second frame, once the header has measured.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (dir, view, cx)
}

fn shown_text(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn the_chip_opens_the_verdict_under_it_and_a_click_elsewhere_closes_it(cx: &mut TestAppContext) {
    let (_dir, view, cx) = open_stacked(cx, crate::pull_request::stacked_fixture().unwrap());
    let chip = cx.debug_bounds("orchestrator-stack-chip").unwrap();
    let title = cx.debug_bounds("orchestrator-detail-title").unwrap();
    assert!(chip.bottom() <= title.top());
    assert!(!shown_text(cx, "orchestrator-stack-popover"));

    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    let popover = cx.debug_bounds("orchestrator-stack-popover").unwrap();
    assert!(popover.top() >= chip.bottom());
    let window = cx.update(|window, _| window.viewport_size());
    assert!(popover.right() <= window.width && popover.bottom() <= window.height);
    // Bottom to top, every layer read.
    let steps: Vec<_> = [
        "orchestrator-stack-layer-0",
        "orchestrator-stack-layer-1",
        "orchestrator-stack-layer-2",
        "orchestrator-stack-layer-3",
    ]
    .into_iter()
    .map(|selector| cx.debug_bounds(selector).unwrap())
    .collect();
    assert!(steps.windows(2).all(|pair| pair[0].left() < pair[1].left()));
    assert!(steps.iter().all(|step| popover.contains(&step.center())));

    // Pressing the chip again closes it rather than reopening it.
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(!shown_text(cx, "orchestrator-stack-popover"));
    cx.update(|_, cx| assert_eq!(view.read(cx).stack.open, None));

    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(shown_text(cx, "orchestrator-stack-popover"));
    cx.simulate_click(point(px(4.), window.height - px(4.)), Modifiers::none());
    cx.run_until_parked();
    assert!(!shown_text(cx, "orchestrator-stack-popover"));
}

#[gpui::test]
fn the_verdict_names_the_lowest_layer_that_blocks_and_opens_it(cx: &mut TestAppContext) {
    let (_dir, view, cx) = open_stacked(cx, crate::pull_request::stacked_fixture().unwrap());
    cx.update(|_, cx| {
        let view = view.read(cx);
        let pr = view.open_pull_request().unwrap();
        let (stack, position) = pr.stack().unwrap();
        assert_eq!(
            stack.verdict(position),
            crate::pull_request::Verdict::Blocked {
                number: 1362,
                blocker: crate::pull_request::Blocker::ChecksFailed,
            }
        );
    });
    let chip = cx.debug_bounds("orchestrator-stack-chip").unwrap();
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    let verdict = cx.debug_bounds("orchestrator-stack-verdict").unwrap();
    let blocker = cx.debug_bounds("orchestrator-stack-layer-1").unwrap();
    assert!(verdict.bottom() <= blocker.top());
    cx.simulate_click(blocker.center(), Modifiers::none());
    cx.run_until_parked();
    let opened = listed(1362).key.canonical();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.detail.as_ref().map(|d| d.key.as_str()),
            Some(opened.as_str())
        );
        assert_eq!(view.stack.open, None);
    });
}

#[gpui::test]
fn a_queued_pull_request_outside_a_stack_shows_its_place_in_the_queue(cx: &mut TestAppContext) {
    let mut looked_up = crate::pull_request::fixture().unwrap();
    looked_up.number = 1364;
    looked_up.url = "https://github.com/example/project/pull/1364".into();
    looked_up.queue = Some(crate::pull_request::QueueEntry {
        state: crate::pull_request::QueueState::AwaitingChecks,
        position: 2,
        eta: Some(600),
    });
    let (_dir, _view, cx) = open_stacked(cx, looked_up);
    assert!(!shown_text(cx, "orchestrator-stack-chip"));
    let tag = cx.debug_bounds("orchestrator-queue-tag").unwrap();
    let title = cx.debug_bounds("orchestrator-detail-title").unwrap();
    assert!(tag.bottom() <= title.top());
}

#[gpui::test]
fn escape_closes_the_stack_popover_before_the_page(cx: &mut TestAppContext) {
    let (_dir, view, cx) = open_stacked(cx, crate::pull_request::stacked_fixture().unwrap());
    let chip = cx.debug_bounds("orchestrator-stack-chip").unwrap();
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focus = view.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.stack.open, None);
        assert!(view.detail.is_some(), "the page stays open");
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(view.read(cx).detail.is_none()));
}
