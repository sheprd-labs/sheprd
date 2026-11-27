//! Explicit rich-text links use the pane's existing document-opening policy.
//! Paths from SSH conversations stay visible and never address this laptop.

use crate::chat::view::rich::*;
use crate::{
    chat::{Target, rich::remote_content},
    terminal::{PaneLink, RowLink, file_target},
};
use sheprd_chat_protocol::Cursor;
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub(in crate::chat::view) enum Mode {
    Configured,
    Attachment,
}

impl Mode {
    /// A file card supplies its own affordance; its selectable filename reads
    /// as a label while preserving the same link target and click handling.
    pub(super) fn highlight(self, mut style: HighlightStyle) -> HighlightStyle {
        if matches!(self, Self::Attachment) {
            style.color = None;
            style.underline = None;
            style.font_weight.get_or_insert(FontWeight::MEDIUM);
        }
        style
    }
}

#[derive(Clone)]
pub(in crate::chat::view) struct Links {
    view: WeakEntity<SheprdWindow>,
    target: Option<Target>,
    cursor: Option<Cursor>,
    selection_epoch: u64,
    local: bool,
    pub(in crate::chat::view) mode: Mode,
}

impl Links {
    pub(in crate::chat::view) fn new(
        view: &SheprdWindow,
        pane_id: &str,
        cx: &Context<SheprdWindow>,
    ) -> Self {
        let pane = view.chats.pane(pane_id);
        Self {
            view: cx.entity().downgrade(),
            mode: Mode::Configured,
            target: pane.map(|pane| pane.target.clone()),
            cursor: pane.and_then(|pane| pane.session.since()),
            selection_epoch: view.selection_epoch,
            local: pane.is_some_and(|pane| pane.target.is_local()),
        }
    }

    pub(super) fn content<'a>(&self, content: &'a [Inline]) -> Cow<'a, [Inline]> {
        if self.local {
            Cow::Borrowed(content)
        } else {
            remote_content(content)
        }
    }

    pub(in crate::chat::view) fn open_attachment(&self, destination: &str, cx: &mut App) {
        self.open_with(destination, true, cx);
    }

    pub(super) fn open(&self, destination: &str, window: &mut Window, cx: &mut App) {
        if let Some(target) = crate::chat::screen_link::target(destination) {
            self.open_screen(&target, window, cx);
            return;
        }
        self.open_with(destination, matches!(self.mode, Mode::Attachment), cx);
    }

    fn open_with(&self, destination: &str, system: bool, cx: &mut App) {
        if let Ok(url) = crate::browser::WebUrl::try_from(destination) {
            cx.open_url(url.as_str());
            return;
        }
        self.with_file(destination, cx, |view, file, cx| {
            view.open_file_link(
                file,
                !system && view.config.open_files_in == crate::config::FileTarget::Editor,
                cx,
            );
        });
    }

    /// Shows the workspace or pane a `sheprd://` link names, in the Herdr
    /// this window shows the chat from, under the same fence as a file: a
    /// herdr-core terminal (`core:t7`) in its tab, anything else through
    /// Herdr 1. An SSH chat's ids are its host's, so its links stay inert.
    fn open_screen(
        &self,
        screen: &crate::OwnedNavigationTarget,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(target) = self.target.as_ref().filter(|_| self.local) else {
            return;
        };
        let _ = self.view.update(cx, |view, cx| {
            let current = view
                .chats
                .pane(&target.pane_id)
                .is_some_and(|pane| pane.is_current(target, self.cursor));
            if !current || view.selected_is_remote() || view.selection_epoch != self.selection_epoch
            {
                return;
            }
            if let crate::NavigationTarget::Pane(pane) = screen
                && let Some(terminal) = crate::chat::core_pane::terminal_of(pane)
            {
                view.show_local_core_terminal(&terminal, window, cx);
                return;
            }
            let endpoint = view.endpoints[view.selected_endpoint].id.clone();
            if view.navigate_endpoint(&endpoint, screen.as_deref(), cx)
                && let crate::NavigationTarget::Pane(pane) = screen
            {
                view.reveal_herdr_pane(pane);
                cx.notify();
            }
        });
    }

    /// Selects an attachment's file in the system file manager, under the
    /// same session fence as opening it.
    pub(in crate::chat::view) fn reveal_attachment(&self, destination: &str, cx: &mut App) {
        self.with_file(destination, cx, |view, file, cx| {
            view.reveal_file_link(file, cx);
        });
    }

    /// Runs `act` with the local file `destination` names, only while this
    /// pane's conversation and the window's selection are still current.
    fn with_file(
        &self,
        destination: &str,
        cx: &mut App,
        act: impl FnOnce(&mut SheprdWindow, crate::window::FileLink, &mut Context<SheprdWindow>),
    ) {
        let Some(target) = self.target.as_ref().filter(|_| self.local) else {
            return;
        };
        let Some(link) = file_target(destination) else {
            return;
        };
        let _ = self.view.update(cx, |view, cx| {
            let current = view
                .chats
                .pane(&target.pane_id)
                .is_some_and(|pane| pane.is_current(target, self.cursor));
            if !current || view.selected_is_remote() || view.selection_epoch != self.selection_epoch
            {
                return;
            }
            let Some(file) = view.file_link(PaneLink {
                pane_id: target.pane_id.clone(),
                link: RowLink {
                    target: link,
                    row: 0,
                    columns: 0..0,
                    continued: Vec::new(),
                },
            }) else {
                return;
            };
            act(view, file, cx);
        });
    }
}

#[cfg(test)]
mod tests;
