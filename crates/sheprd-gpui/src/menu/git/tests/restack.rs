//! Once the layer right below the focused branch's pull request merged, the
//! Git popup offers to restack the branch onto the stack's base, after a
//! confirmation, through the Git worker.
use super::{Page, REPO_KEY, Row};
use crate::{
    git::{Action, Git, Status},
    pull_request::{Input, restack_fixture, stacked_fixture},
};
use gpui::{TestAppContext, point, px};
use std::time::Instant;

fn input() -> Input {
    Input {
        checkout: None,
        repo_key: Some(REPO_KEY.into()),
        branch: "develop".into(),
    }
}

fn open_with(
    cx: &mut TestAppContext,
    pr: crate::pull_request::PullRequest,
) -> (
    gpui::Entity<crate::SheprdWindow>,
    &mut gpui::VisualTestContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(input(), Status::default());
            view.menu.github = crate::github::Auth::connected_fixture();
            view.seed_pr(input(), pr, Instant::now());
            view.open_git_menu(point(px(860.), px(20.)), window, cx);
        })
    });
    (view, cx)
}

#[gpui::test]
fn a_branch_right_above_a_merged_layer_restacks_after_confirming(cx: &mut TestAppContext) {
    let (view, cx) = open_with(cx, restack_fixture().unwrap());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let rows: Vec<_> = view.git_rows().into_iter().collect();
            let restack = rows.iter().find(|(row, _)| *row == Row::Restack);
            assert_eq!(
                restack.map(|(_, label)| label.replace(['\u{2068}', '\u{2069}'], "")),
                Some("Restack onto main...".into())
            );
            view.activate_git_row(Row::Restack, window, cx);
            assert_eq!(view.menu.page, Some(Page::GitRestack));
            assert!(
                view.git.running().is_none(),
                "nothing runs before confirming"
            );
            view.submit_git_restack(cx);
            assert_eq!(view.menu.page, Some(Page::Git));
            assert_eq!(
                view.git.running(),
                Some(&Action::Restack {
                    below: format!("{:040x}", 1361),
                    base: "main".into(),
                })
            );
        })
    });
}

#[gpui::test]
fn a_branch_with_an_open_layer_below_is_not_offered_a_restack(cx: &mut TestAppContext) {
    let (view, cx) = open_with(cx, stacked_fixture().unwrap());
    cx.update(|_, cx| {
        let rows = view.read(cx).git_rows();
        assert!(rows.iter().all(|(row, _)| *row != Row::Restack));
    });
}

#[gpui::test]
fn a_restack_whose_target_changed_since_opening_is_refused(cx: &mut TestAppContext) {
    let (view, cx) = open_with(cx, restack_fixture().unwrap());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.activate_git_row(Row::Restack, window, cx);
            assert_eq!(view.menu.page, Some(Page::GitRestack));
            // The lookup moves on: the layer below is open again.
            view.seed_pr(input(), stacked_fixture().unwrap(), Instant::now());
            view.submit_git_restack(cx);
            assert!(view.git.running().is_none(), "nothing runs");
            assert!(view.menu.error.is_some());
            assert_eq!(view.menu.page, Some(Page::GitRestack));
        })
    });
}
