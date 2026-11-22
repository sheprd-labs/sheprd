use super::{
    window::{checked, merged_window},
    *,
};
use crate::{menu::Page, sidebar::layout_tests::fixture_window};

#[gpui::test]
fn the_broom_asks_once_for_every_merged_worktree(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let candidate = merged_window(view);
            // Only the verified local daemon's checkouts are looked up.
            assert!(view.merged_worktrees().is_empty());
            view.live.local_daemon_peer = true;
            assert_eq!(view.merged_worktrees(), [candidate]);
            view.open_merged_sweep(window, cx);
            assert_eq!(view.menu.page, Some(Page::RemoveMerged));
            // Asking removes nothing yet.
            assert!(view.merged_cleanup.sweep.is_none());
            assert!(view.merged_cleanup.probe.is_none());
        })
    });
}

#[gpui::test]
fn nothing_merged_says_so_instead_of_asking(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            assert!(view.merged_worktrees().is_empty());
            view.open_merged_sweep(window, cx);
            assert_eq!(view.menu.page, None);
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash),
                Some(&crate::window::Flash::success(
                    "No merged worktrees to remove"
                ))
            );
        })
    });
}

#[gpui::test]
fn a_sweep_removes_the_focused_worktree_without_asking_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let candidate = merged_window(view);
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.focused_workspace_id = Some(candidate.workspace.clone());
            view.live.snapshot = Some(Arc::new(snapshot));
            assert!(!view.config.remove_merged_worktrees);
            view.merged_cleanup.sweep = Some(crate::merged_cleanup::sweep::Sweep::default());
            view.merged_cleanup.probe = Some(checked(&candidate));
            view.finish_merged_probe(window, cx);
            // No delete dialog: the sweep was confirmed. This fixture cannot
            // send, so the removal counts as kept and waits for a retry.
            assert_eq!(view.menu.page, None);
            let sweep = view.merged_cleanup.sweep.as_ref().unwrap();
            assert_eq!((sweep.skipped, sweep.done), (1, 1));
            assert!(
                view.merged_cleanup
                    .ledger
                    .waiting(&candidate, Instant::now())
            );
        })
    });
}

#[gpui::test]
fn a_finished_sweep_says_how_many_stayed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.merged_cleanup.sweep = Some(crate::merged_cleanup::sweep::Sweep {
                skipped: 2,
                done: 3,
                total: 3,
                ..Default::default()
            });
            view.menu.page = Some(Page::RemoveMerged);
            view.merged_cleanup.sweep.as_mut().unwrap().device = view.sweep_device();
            // A list still being scanned keeps the sweep going.
            view.merged_cleanup.idle = true;
            view.end_merged_sweep(window, cx);
            assert!(view.merged_cleanup.sweep.is_some());
            view.merged_cleanup.kept.idle = true;
            view.end_merged_sweep(window, cx);
            assert!(view.merged_cleanup.sweep.is_none());
            // Its progress dialog closes with it.
            assert_eq!(view.menu.page, None);
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash),
                Some(&crate::window::Flash::warning(crate::i18n::fl!(
                    "merged-sweep-kept",
                    count = 2
                )))
            );
        })
    });
}

#[gpui::test]
fn confirming_keeps_the_dialog_open_as_progress(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            merged_window(view);
            view.live.local_daemon_peer = true;
            view.open_merged_sweep(window, cx);
            view.confirm_merged_sweep(window, cx);
            assert_eq!(view.menu.page, Some(Page::RemoveMerged));
            let sweep = view.merged_cleanup.sweep.as_ref().unwrap();
            assert_eq!((sweep.done, sweep.total), (0, 1));
            // Its check is running, and the dialog names it.
            assert!(view.merged_cleanup.current().is_some());
            // Confirming again starts nothing new.
            view.confirm_merged_sweep(window, cx);
            assert_eq!(view.merged_cleanup.sweep.as_ref().unwrap().total, 1);
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("merged-sweep-bar").is_some());
    assert!(cx.debug_bounds("merged-sweep-count").is_some());
    assert!(cx.debug_bounds("merged-sweep-submit").is_none());
}

#[gpui::test]
fn a_held_enter_never_confirms_the_sweep(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let enter = |cx: &mut gpui::VisualTestContext, is_held| {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("enter").unwrap(),
                    is_held,
                    prefer_character_input: false,
                }),
                cx,
            );
        });
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            merged_window(view);
            view.live.local_daemon_peer = true;
            view.open_merged_sweep(window, cx);
        })
    });
    enter(cx, true);
    assert!(view.read_with(cx, |view, _| view.merged_cleanup.sweep.is_none()));
    enter(cx, false);
    assert!(view.read_with(cx, |view, _| view.merged_cleanup.sweep.is_some()));
}

#[gpui::test]
fn another_device_or_connection_stops_the_sweep(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            merged_window(view);
            view.live.local_daemon_peer = true;
            view.open_merged_sweep(window, cx);
            view.confirm_merged_sweep(window, cx);
            view.stop_merged_sweep_elsewhere(window, cx);
            assert!(view.merged_cleanup.sweep.is_some());
            // A reconnect is another connection, which was never confirmed.
            view.endpoints[view.selected_endpoint].generation += 1;
            view.stop_merged_sweep_elsewhere(window, cx);
            assert!(view.merged_cleanup.sweep.is_none());
            assert_eq!(view.menu.page, None);
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash),
                Some(&crate::window::Flash::warning(crate::i18n::fl!(
                    "merged-sweep-stopped"
                )))
            );
        })
    });
}

#[gpui::test]
fn a_handled_worktree_is_never_checked_again_in_the_sweep(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let candidate = merged_window(view);
            view.live.local_daemon_peer = true;
            view.open_merged_sweep(window, cx);
            view.confirm_merged_sweep(window, cx);
            // The ledger forgets it (a retry came due, or it was evicted),
            // but the sweep remembers, so it scans to its end.
            view.merged_cleanup.probe = None;
            view.merged_cleanup.skipped(&candidate);
            view.merged_cleanup.ledger = Ledger::default();
            view.merged_cleanup.next_scan = None;
            view.merged_cleanup.kept.idle = true;
            view.update_merged_cleanup(window, cx);
            assert!(view.merged_cleanup.probe.is_none());
            assert!(view.merged_cleanup.idle);
            view.update_merged_cleanup(window, cx);
            assert!(view.merged_cleanup.sweep.is_none());
        })
    });
}

#[gpui::test]
fn a_kept_removal_counts_only_once_git_answers(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let candidate = merged_window(view);
            view.merged_cleanup.sweep = Some(crate::merged_cleanup::sweep::Sweep {
                total: 2,
                ..Default::default()
            });
            view.merged_cleanup.kept.idle = true;
            view.merged_cleanup
                .kept
                .track_removal("kept-1".into(), candidate.clone());
            assert!(!view.merged_cleanup.kept.settled());
            // Another request's answer is not this one's.
            view.finish_merged_kept_removal("kept-2", true);
            assert!(!view.merged_cleanup.kept.settled());
            view.finish_merged_kept_removal("kept-1", false);
            assert!(view.merged_cleanup.kept.settled());
            let sweep = view.merged_cleanup.sweep.as_ref().unwrap();
            assert_eq!((sweep.done, sweep.skipped), (1, 1));
            assert!(sweep.handled.contains(&candidate));
        })
    });
}
