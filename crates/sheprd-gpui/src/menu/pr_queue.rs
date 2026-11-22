//! The Git popup's merge queue confirmation: adding the focused branch's pull
//! request (with the open layers of its stack below it) to its base branch's
//! merge queue, or taking it out. Also how `sheprd pr merge` and
//! `sheprd pr queue` open this dialog or the merge one, which the user
//! confirms; nothing is sent until they do.
use super::{Page, git::Row};
use crate::{
    SheprdWindow,
    i18n::fl,
    pr_actions::{Action, Landing},
};
use gpui::{prelude::*, *};
use sheprd_control_protocol::protocol::hosts::QueueOp;

/// A dialog a command opens on the focused branch's pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrChange {
    /// The merge dialog, which lands the open layers below on a stack.
    Merge,
    Queue(QueueOp),
}

impl SheprdWindow {
    /// Opens `change`'s dialog in the Git popup, where the user confirms.
    /// False when another menu or dialog is open, or the focused branch is
    /// not one the popup follows.
    pub(crate) fn ask_pr_change(
        &mut self,
        change: PrChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() {
            return false;
        }
        // Where the titlebar's Git button last painted, as its shortcut opens
        // it; zen mode hides the button, so it drops from the corner.
        let anchor = if self.zen_active() {
            point(window.viewport_size().width - px(24.), px(24.))
        } else {
            self.git_anchor.get()
        };
        self.open_git_menu(anchor, window, cx);
        if self.menu.page != Some(Page::Git) {
            return false;
        }
        match change {
            PrChange::Merge => self.open_pr_merge(),
            PrChange::Queue(change) => self.open_pr_queue(change),
        }
        self.menu.asked_by_command = true;
        cx.notify();
        true
    }

    pub(super) fn open_pr_queue(&mut self, change: QueueOp) {
        self.menu.page = Some(Page::PrQueue);
        self.menu.asked_by_command = false;
        self.menu.error = None;
        self.menu.queue_change = Some(change);
        self.menu.merge_target = self.pr_actions.target().cloned();
    }

    /// The confirmation itself, only for the head the dialog was opened with.
    pub(super) fn submit_pr_queue(&mut self, cx: &mut Context<Self>) {
        let result = (|| -> crate::Result<()> {
            let change = self
                .menu
                .queue_change
                .ok_or(sheprd_github::pr_actions::Error::Target)?;
            if self.menu.merge_target.is_none()
                || self.menu.merge_target.as_ref() != self.pr_actions.target()
            {
                return Err(sheprd_github::pr_actions::Error::MergeChanged.into());
            }
            let token = self
                .pr_token()
                .ok_or(sheprd_github::Error::Authentication)?;
            let action = match change {
                QueueOp::Add => Action::Land(Landing::Queue),
                QueueOp::Remove => Action::Dequeue,
            };
            Ok(self.pr_actions.start(action, &[], token)?)
        })();
        match result {
            Ok(()) => {
                self.menu.error = None;
                self.menu.merge_target = None;
                self.menu.queue_change = None;
                self.menu.page = Some(Page::Git);
            }
            Err(error) => self.menu.error = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(super) fn render_pr_queue(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let change = self.menu.queue_change.unwrap_or(QueueOp::Add);
        let pr = self.git_open_pull_request();
        let numbers = pr
            .as_deref()
            .map(|pr| {
                pr.lands()
                    .iter()
                    .map(|number| format!("#{number}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        // The commit the dialog was opened for, which the queue names.
        let head = self
            .menu
            .merge_target
            .as_ref()
            .map(|target| target.head().chars().take(12).collect::<String>())
            .unwrap_or_default();
        let (title, body, verb) = match change {
            QueueOp::Add => (
                fl!("pr-stack-dialog-add-title"),
                fl!(
                    "pr-stack-dialog-add-body",
                    numbers = numbers.as_str(),
                    head = head.as_str()
                ),
                fl!("pr-stack-dialog-add"),
            ),
            QueueOp::Remove => (
                fl!("pr-stack-dialog-remove-title"),
                fl!("pr-stack-dialog-remove-body"),
                fl!("pr-stack-dialog-remove"),
            ),
        };
        let armed = self.pr_actions.running().is_none() && self.menu.merge_target.is_some();
        div()
            .flex()
            .flex_col()
            .child(self.pr_dialog_header(Row::Merge, title))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .px(px(16.))
                    .py(px(12.))
                    .child(
                        div()
                            .debug_selector(|| "pr-queue-body".into())
                            .text_color(rgb(theme.muted))
                            .child(crate::sidebar::label_text(&body)),
                    )
                    .children(self.pr_dialog_error()),
            )
            .child(
                self.pr_footer().child(self.pr_back_button(cx)).child(
                    self.pr_button("pr-queue-confirm", verb, armed)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.submit_pr_queue(cx);
                        })),
                ),
            )
    }
}
