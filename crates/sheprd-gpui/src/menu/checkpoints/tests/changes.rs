//! The dialog's commit versions section.

use super::local;
use crate::{
    checkpoint::{Changes, Listing},
    menu::page::WorkspaceMenuAction,
};
use sheprd_vcs::evolution::{Commit, History, Marker, Match, Outcome};

fn sha(digit: char) -> String {
    digit.to_string().repeat(40)
}

/// `two` (b) amended from c in Codex's turn, then rebased from a since
/// the last turn; d dropped outside a turn.
pub(super) fn history_changes() -> Changes {
    let marker = |old, new: Option<char>, by: Option<&str>, how| Marker {
        time: 0,
        old: sha(old),
        outcome: new.map_or(Outcome::Dropped, |new| Outcome::Replaced {
            new: sha(new),
            how,
        }),
        by: by.map(str::to_owned),
    };
    Changes::Ready(Some(History {
        stack: vec![
            Commit {
                sha: sha('e'),
                patch_id: None,
                change_id: None,
                subject: "one".into(),
            },
            Commit {
                sha: sha('b'),
                patch_id: None,
                change_id: None,
                subject: "two".into(),
            },
        ],
        markers: vec![
            marker('c', Some('a'), Some("Codex"), Match::Subject),
            marker('d', None, None, Match::Subject),
            marker('a', Some('b'), None, Match::Patch),
        ],
        pending: 1,
    }))
}

fn open(
    view: &mut crate::SheprdWindow,
    changes: Changes,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::SheprdWindow>,
) {
    local(view);
    view.open_workspace_menu("w4", Default::default(), window, cx);
    view.activate_workspace_menu(WorkspaceMenuAction::Checkpoints, window, cx);
    let checkpoints = view.checkpoints.view.as_mut().unwrap();
    checkpoints.listing = Listing::Ready(Vec::new());
    checkpoints.changes = changes;
    cx.notify();
}

#[gpui::test]
fn rewritten_commits_are_listed_with_their_versions(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| view.update(cx, |view, cx| open(view, history_changes(), window, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("checkpoint-change-bbbbbbb").is_some());
    assert!(
        cx.debug_bounds("checkpoint-change-eeeeeee").is_none(),
        "never rewritten"
    );
    assert!(cx.debug_bounds("checkpoint-changes-none").is_none());
}

#[gpui::test]
fn a_branch_with_nothing_rewritten_says_so(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let quiet = Changes::Ready(Some(History {
        stack: Vec::new(),
        markers: Vec::new(),
        pending: 0,
    }));
    cx.update(|window, cx| view.update(cx, |view, cx| open(view, quiet, window, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("checkpoint-changes-none").is_some());

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            open(view, Changes::Failed("broken".into()), window, cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("checkpoint-changes-error").is_some());
}
