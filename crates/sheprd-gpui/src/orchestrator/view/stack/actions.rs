//! The stack popover's footer: merging the stack, once every open layer up to
//! this one can merge, and adding it to or taking it out of the merge queue.
//! Each opens the view's confirmation, which names the pull requests that
//! land and the head commit seen; nothing is sent from here.

use super::{OrchestratorView, Popover};
use crate::{
    i18n::fl,
    orchestrator::view::dispatch::{Confirm, merge_action},
    pr_actions::{Action, Landing, Target},
    pull_request::{MergeMethod, PullRequest, Stack, State, Verdict},
};
use gpui::{prelude::*, *};

/// What the footer asks to confirm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::orchestrator::view) enum Change {
    /// Merge with the open layers below, with this method.
    Merge(MergeMethod),
    Enqueue,
    Dequeue,
}

impl From<Change> for Action {
    fn from(change: Change) -> Self {
        match change {
            Change::Merge(method) => Self::Land(Landing::Stack(method)),
            Change::Enqueue => Self::Land(Landing::Queue),
            Change::Dequeue => Self::Dequeue,
        }
    }
}

impl OrchestratorView {
    /// The footer's buttons, or what is running, when the pull request can
    /// be acted on from here.
    pub(super) fn stack_actions(
        &self,
        pr: &PullRequest,
        stack: &Stack,
        position: u64,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let look = &self.look;
        if let Some(running) = self.pr.running() {
            return Some(footer(look).child(look.muted(running.running_label())));
        }
        let target = self.pr.target().filter(|_| writable(pr))?.clone();
        let lands: Vec<u64> = stack.landing(position).map(|layer| layer.number).collect();
        // Queueing lands every open layer below too, so it is offered only
        // when the confirmation can name them all.
        let queueable = pr.merge_queue && stack.read_through(position);
        let row = footer(look);
        if pr.queue.is_some() {
            let target = target.clone();
            return Some(
                row.child(
                    look.button(
                        "orchestrator-stack-dequeue",
                        fl!("pr-stack-queue-remove"),
                        false,
                    )
                    .debug_selector(|| "orchestrator-stack-dequeue".into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask_landing(Change::Dequeue, target.clone(), Vec::new(), cx);
                    })),
                ),
            );
        }
        let ready = match stack.verdict(position) {
            Verdict::Ready { count } => Some(count),
            _ => None,
        };
        let methods = pr.merge_methods.clone();
        let picking = self.stack.methods;
        let row = row
            .when_some(ready, |row, count| {
                let target = target.clone();
                let lands = lands.clone();
                let methods = methods.clone();
                row.child(
                    look.button(
                        "orchestrator-stack-merge",
                        fl!("pr-stack-merge", count = count),
                        true,
                    )
                    .debug_selector(|| "orchestrator-stack-merge".into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // One method needs no picking.
                        if let [method] = methods.as_slice() {
                            let change = Change::Merge(*method);
                            this.ask_landing(change, target.clone(), lands.clone(), cx);
                        } else {
                            this.stack.methods = !this.stack.methods;
                            cx.notify();
                        }
                    })),
                )
            })
            .when(queueable, |row| {
                let target = target.clone();
                let lands = lands.clone();
                row.child(
                    look.button(
                        "orchestrator-stack-enqueue",
                        fl!("pr-stack-queue-add"),
                        ready.is_none(),
                    )
                    .debug_selector(|| "orchestrator-stack-enqueue".into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask_landing(Change::Enqueue, target.clone(), lands.clone(), cx);
                    })),
                )
            });
        if ready.is_none() && !queueable {
            return None;
        }
        let picker = (picking && ready.is_some() && methods.len() > 1).then(|| {
            footer(look)
                .pt_0()
                .border_t_0()
                .children(methods.iter().copied().map(|method| {
                    let target = target.clone();
                    let lands = lands.clone();
                    look.button(
                        SharedString::from(format!(
                            "orchestrator-stack-merge-{}",
                            method.graphql()
                        )),
                        merge_action(method),
                        false,
                    )
                    .debug_selector(move || {
                        format!("orchestrator-stack-merge-{}", method.graphql())
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let change = Change::Merge(method);
                        this.ask_landing(change, target.clone(), lands.clone(), cx);
                    }))
                }))
        });
        Some(div().flex().flex_col().child(row).children(picker))
    }

    /// The status box's queue button for an open pull request in no stack,
    /// on a branch with a merge queue: taking it out while it is queued,
    /// else adding it. A stacked one's live in its popover.
    pub(in crate::orchestrator::view) fn queue_change(
        &self,
        item: &crate::orchestrator::Item,
    ) -> Option<(Change, Target, Vec<u64>)> {
        item.github_pull_request()?;
        let pr = self.open_pull_request()?;
        if !pr.merge_queue || pr.stack().is_some() || !writable(pr) || self.pr.running().is_some() {
            return None;
        }
        let target = self.pr.target()?.clone();
        Some(match pr.queue {
            Some(_) => (Change::Dequeue, target, Vec::new()),
            None => (Change::Enqueue, target, vec![pr.number]),
        })
    }

    /// Closes the popover and asks to confirm `change` on `target`.
    fn ask_landing(
        &mut self,
        change: Change,
        target: Target,
        lands: Vec<u64>,
        cx: &mut Context<Self>,
    ) {
        self.stack = Popover::default();
        self.ask(
            Confirm::Land {
                change,
                target,
                lands,
            },
            cx,
        );
    }
}

/// Whether merges and queue changes may be offered: open and not a draft.
fn writable(pr: &PullRequest) -> bool {
    pr.state == State::Open && !pr.is_draft
}

fn footer(look: &crate::orchestrator::view::Look) -> Div {
    div()
        .px_3()
        .py_2()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .border_t_1()
        .border_color(rgb(look.theme.active))
}

/// The confirmation's title, what it does, its button, and the button's ID.
pub(in crate::orchestrator::view) fn confirm_text(
    change: Change,
    target: &Target,
    lands: &[u64],
) -> (String, String, String, &'static str) {
    let numbers = lands
        .iter()
        .map(|number| format!("#{number}"))
        .collect::<Vec<_>>()
        .join(", ");
    let head = target.head().get(..12).unwrap_or(target.head());
    match change {
        Change::Merge(method) => (
            fl!("pr-stack-confirm-merge-title", count = lands.len()),
            fl!(
                "pr-stack-confirm-merge-body",
                action = merge_action(method),
                numbers = numbers,
                number = target.number,
                head = head
            ),
            fl!("pr-stack-confirm-merge"),
            "merge-stack",
        ),
        Change::Enqueue => (
            fl!("pr-stack-confirm-queue-title", number = target.number),
            fl!(
                "pr-stack-confirm-queue-body",
                numbers = numbers,
                number = target.number,
                head = head
            ),
            fl!("pr-stack-confirm-queue"),
            "enqueue",
        ),
        Change::Dequeue => (
            fl!("pr-stack-confirm-dequeue-title", number = target.number),
            fl!("pr-stack-confirm-dequeue-body"),
            fl!("pr-stack-confirm-dequeue"),
            "dequeue",
        ),
    }
}
