//! The Git popup's restack: once the layer directly below the focused
//! branch's pull request merged into its GitHub stack's base, the branch
//! still carries that layer's old commits. The confirmation says what moves;
//! the Git worker then replays the branch's own commits onto the base as the
//! remote has it now, locally, and never pushes.
use super::{Page, git::Row};
use crate::{
    SheprdWindow,
    git::{Action, restack::Restacked},
    i18n::fl,
    pull_request::State,
};
use gpui::{prelude::*, *};

/// What a restack moves the focused branch onto, and past.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Restack {
    /// The merged layer's number and head commit.
    pub(super) below: u64,
    pub(super) oid: String,
    /// The stack's base branch.
    pub(super) base: String,
}

impl SheprdWindow {
    /// The restack the focused branch's open pull request calls for, if the
    /// layer directly below it merged.
    pub(super) fn git_restack(&self) -> Option<Restack> {
        let pr = self.git_open_pull_request()?;
        if pr.state != State::Open {
            return None;
        }
        let (stack, position) = pr.stack()?;
        let below = stack.merged_below(position)?;
        Some(Restack {
            below: below.number,
            oid: below.head_oid.clone(),
            base: stack.base.clone(),
        })
    }

    /// Opens the confirmation for the focused checkout's restack, which it
    /// keeps: confirming acts on that one or on nothing.
    pub(super) fn open_git_restack(&mut self) {
        self.menu.page = Some(Page::GitRestack);
        self.menu.error = None;
        self.menu.restack = self.git.tracked().cloned().zip(self.git_restack());
    }

    /// The confirmation itself, only for the checkout and restack the dialog
    /// was opened with: a focus or lookup that moved since is refused.
    pub(super) fn submit_git_restack(&mut self, cx: &mut Context<Self>) {
        if self.git.running().is_some() || self.pr_actions.running().is_some() {
            return;
        }
        let current = self.git.tracked().cloned().zip(self.git_restack());
        let Some((_, restack)) = self
            .menu
            .restack
            .clone()
            .filter(|kept| Some(kept) == current.as_ref())
        else {
            self.menu.error = Some(fl!("git-menu-restack-changed"));
            cx.notify();
            return;
        };
        self.menu.restack = None;
        self.start_git(Action::Restack {
            below: restack.oid,
            base: restack.base,
        });
        if self.menu.error.is_none() {
            self.menu.page = Some(Page::Git);
        }
        cx.notify();
    }

    pub(super) fn render_git_restack(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let restack = self.menu.restack.as_ref().map(|(_, restack)| restack);
        let base = restack
            .as_ref()
            .map_or(String::new(), |restack| restack.base.clone());
        let body = restack.as_ref().map(|restack| {
            let branch = self
                .menu
                .restack
                .as_ref()
                .map(|(input, _)| input.branch.clone())
                .unwrap_or_default();
            fl!(
                "git-menu-restack-body",
                number = restack.below,
                branch = branch.as_str(),
                base = restack.base.as_str()
            )
        });
        let armed = restack.is_some() && self.git.running().is_none();
        div()
            .flex()
            .flex_col()
            .child(self.pr_dialog_header(
                Row::Restack,
                fl!("git-menu-restack-title", base = base.as_str()),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .px(px(16.))
                    .py(px(12.))
                    .children(body.map(|body| {
                        div()
                            .debug_selector(|| "git-restack-body".into())
                            .text_color(rgb(theme.muted))
                            .child(crate::sidebar::label_text(&body))
                    }))
                    .children(self.pr_dialog_error()),
            )
            .child(
                self.pr_footer().child(self.pr_back_button(cx)).child(
                    self.pr_button(
                        "git-restack-confirm",
                        fl!("git-menu-restack-confirm"),
                        armed,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.submit_git_restack(cx);
                    })),
                ),
            )
    }
}

/// What a restack did, as the popup says it.
pub(super) fn restacked_text(restacked: &Restacked) -> String {
    match restacked {
        Restacked::Onto {
            branch,
            remote,
            base,
        } => fl!(
            "git-menu-restacked",
            branch = branch.as_str(),
            remote = remote.as_str(),
            base = base.as_str()
        ),
        Restacked::Unchanged { branch } => {
            fl!("git-menu-restack-unchanged", branch = branch.as_str())
        }
    }
}
