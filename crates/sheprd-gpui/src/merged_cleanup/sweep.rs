//! Removing every merged worktree at once, after one confirmation: the
//! sidebar's broom and the `remove_merged_worktrees` command. The dialog
//! lists what is merged now; confirming runs the automatic cleanup's own
//! checks over each, one at a time, whether or not that cleanup is on. A
//! checkout with uncommitted files, commits past the merge, a busy agent, or
//! an archive script stays, and the sweep says how many stayed. The dialog
//! stays open with a bar of how many are done until the last one is, as
//! Herdr answers nothing else while Git removes a checkout; dismissing it
//! leaves the sweep running, and the broom shows it again.
//!
//! The confirmation covers one device's connection: switching devices or
//! reconnecting stops the sweep rather than carrying it to another list.

use super::{Candidate, Ledger, candidates, kept::kept_candidates_on};
use crate::{
    SheprdWindow,
    i18n::fl,
    menu::Page,
    progress::{Progress, bar},
    pull_request::Origin,
    window::Flash,
};
use gpui::{prelude::*, *};

/// Names the dialog lists before it summarizes the rest.
const LISTED: usize = 8;

/// A confirmed pass over every merged worktree.
#[derive(Debug, Default)]
pub(crate) struct Sweep {
    /// The device's endpoint ID and connection generation it was confirmed
    /// for; any other stops it.
    pub(super) device: (String, u64),
    /// Merged worktrees this sweep removed, sent away, or left in place, so
    /// none is checked twice whatever the cleanup's ledger forgets.
    pub(super) handled: Vec<Candidate>,
    /// Merged worktrees left in place, for the summary.
    pub(super) skipped: usize,
    /// Merged worktrees removed or left in place so far.
    pub(super) done: usize,
    /// Merged worktrees listed when the sweep was confirmed.
    pub(super) total: usize,
}

impl SheprdWindow {
    /// The selected device's linked worktrees whose pull request is merged:
    /// the daemon's, then those the app keeps for Herdr 2. The same lists the
    /// cleanup checks, so the count matches what a sweep tries.
    pub(crate) fn merged_worktrees(&self) -> Vec<Candidate> {
        let cache = &self.menu.pr_cache;
        let mut found = Vec::new();
        if self.live.status.is_connected()
            && self.pr_origin() == Some(Origin::Local)
            && let Some(snapshot) = &self.live.snapshot
        {
            found.extend(candidates(snapshot, |input| cache.get(input)));
        }
        let core = self.sessions(self.selected_endpoint).core;
        if let (Some(core), Some(view)) = (core, self.endpoint_pr_view(self.selected_endpoint)) {
            found.extend(kept_candidates_on(
                &core.workspaces,
                self.selected_is_remote(),
                |input| cache.peek_input(&view, input),
            ));
        }
        found
    }

    /// Asks before removing every merged worktree, or says there is none.
    /// While a sweep runs, shows its progress again.
    pub(crate) fn open_merged_sweep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.merged_cleanup.sweep.is_none() && self.merged_worktrees().is_empty() {
            self.show_flash(Flash::success(fl!("merged-sweep-none")), cx);
            return;
        }
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::RemoveMerged);
        cx.notify();
    }

    /// Starts the sweep. Earlier refusals are forgotten, so each merged
    /// worktree is checked again now.
    pub(super) fn confirm_merged_sweep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.merged_cleanup.sweep.is_some() {
            return;
        }
        let total = self.merged_worktrees().len();
        let device = self.sweep_device();
        let cleanup = &mut self.merged_cleanup;
        cleanup.sweep = Some(Sweep {
            device,
            total,
            ..Sweep::default()
        });
        cleanup.ledger = Ledger::default();
        cleanup.next_scan = None;
        cleanup.idle = false;
        cleanup.kept.reset_for_sweep();
        self.update_merged_cleanup(window, cx);
    }

    /// The selected device, as a sweep is tied to it.
    pub(super) fn sweep_device(&self) -> (String, u64) {
        let endpoint = &self.endpoints[self.selected_endpoint];
        (endpoint.id.clone(), endpoint.generation)
    }

    /// Stops a sweep once another device, or another connection to its own,
    /// is selected: its confirmation named this device's worktrees only.
    pub(super) fn stop_merged_sweep_elsewhere(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let device = self.sweep_device();
        if self
            .merged_cleanup
            .sweep
            .as_ref()
            .is_none_or(|sweep| sweep.device == device)
        {
            return;
        }
        self.merged_cleanup.sweep = None;
        if self.menu.page == Some(Page::RemoveMerged) {
            self.dismiss_menu(window, cx);
        }
        self.show_flash(Flash::warning(fl!("merged-sweep-stopped")), cx);
    }

    /// Ends the sweep once neither list has a merged worktree left to check
    /// and nothing is in flight, saying how many stayed.
    pub(super) fn end_merged_sweep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cleanup = &self.merged_cleanup;
        let done = cleanup.sweep.is_some()
            && cleanup.idle
            && cleanup.probe.is_none()
            && cleanup.removing.is_none()
            && cleanup.kept.settled();
        if !done {
            return;
        }
        let Some(sweep) = self.merged_cleanup.sweep.take() else {
            return;
        };
        if self.menu.page == Some(Page::RemoveMerged) {
            self.dismiss_menu(window, cx);
        }
        if sweep.skipped > 0 {
            let flash = Flash::warning(fl!("merged-sweep-kept", count = sweep.skipped));
            self.show_flash(flash, cx);
        }
        cx.notify();
    }

    pub(crate) fn merged_sweep_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            // The Enter that chose the command in the palette may still be
            // held: consent to delete needs a fresh press.
            "enter" if event.is_held => {}
            "enter" => self.confirm_merged_sweep(window, cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    pub(crate) fn render_merged_sweep(&self, cx: &mut Context<Self>) -> Div {
        if let Some(sweep) = &self.merged_cleanup.sweep {
            return self.render_sweep_progress(sweep, cx);
        }
        let theme = &self.theme;
        let merged = self.merged_worktrees();
        let rest = merged.len().saturating_sub(LISTED);
        let names = merged.iter().take(LISTED).map(|candidate| {
            div()
                .truncate()
                .child(format!("#{} {}", candidate.number, candidate.input.branch))
        });
        div()
            .debug_selector(|| "merged-sweep".into())
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("merged-sweep-title", count = merged.len())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .text_color(rgb(theme.subtext()))
                    .children(names)
                    .when(rest > 0, |list| {
                        list.child(fl!("merged-sweep-more", count = rest))
                    }),
            )
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("merged-sweep-note")),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("merged-sweep-cancel")
                            .debug_selector(|| "merged-sweep-cancel".into())
                            .p(px(6.))
                            .cursor_pointer()
                            .child(fl!("merged-sweep-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("merged-sweep-submit")
                            .debug_selector(|| "merged-sweep-submit".into())
                            .p(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .text_color(crate::menu::danger(theme))
                            .cursor_pointer()
                            .child(fl!("merged-sweep-submit"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_merged_sweep(window, cx);
                            })),
                    ),
            )
    }

    /// The running sweep: how many are done, the one in hand, and a button
    /// that hides the dialog without stopping the sweep.
    fn render_sweep_progress(&self, sweep: &Sweep, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let total = sweep.total.max(sweep.done).max(1);
        let current = self
            .merged_cleanup
            .current()
            .map(|candidate| format!("#{} {}", candidate.number, candidate.input.branch));
        div()
            .debug_selector(|| "merged-sweep-progress".into())
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(fl!("merged-sweep-running")),
                    )
                    .child(
                        div()
                            .debug_selector(|| "merged-sweep-count".into())
                            .text_color(rgb(theme.subtext()))
                            .child(fl!(
                                "merged-sweep-count",
                                done = sweep.done.min(total),
                                total = total
                            )),
                    ),
            )
            .child(bar(
                "merged-sweep-bar",
                Progress::Working(sweep.done as f32 / total as f32),
                crate::menu::accent(theme).into(),
                rgb(theme.active).into(),
            ))
            .children(current.map(|current| {
                div()
                    .truncate()
                    .text_color(rgb(theme.subtext()))
                    .child(current)
            }))
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("merged-sweep-running-note")),
            )
            .child(
                div().flex().justify_end().child(
                    div()
                        .id("merged-sweep-hide")
                        .debug_selector(|| "merged-sweep-hide".into())
                        .p(px(6.))
                        .cursor_pointer()
                        .child(fl!("merged-sweep-hide"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_menu(window, cx);
                        })),
                ),
            )
    }
}
