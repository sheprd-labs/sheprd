//! Copy buttons: by an agent message, and in a rich block. Both copy only
//! text and confirm in a flash.

use super::layout::layout_for;
use crate::{SheprdWindow, config::corners, i18n::fl, window::Flash};
use gpui::{prelude::*, *};

/// Where a message's copy button may sit, by what the message shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyPlacement {
    /// Plain prose: the layout may float the button over the message.
    Floating,
    /// Code, a table, a picture, or a control, which a floating button
    /// would cover: the layout keeps the button below the message.
    Footer,
}

/// A rich block's copy button: its name for tests, and the messages of its
/// label and of what its flash says.
#[derive(Clone, Copy)]
pub(super) struct RichCopy {
    name: &'static str,
    label: &'static str,
    copied: &'static str,
}

// keep-sorted start
pub(super) const ANSWERS: RichCopy = RichCopy {
    name: "chat-copy-answers",
    label: "chat-copy-answers-label",
    copied: "chat-copy-answers-done",
};
pub(super) const CODE: RichCopy = RichCopy {
    name: "chat-copy-code",
    label: "chat-copy-code-label",
    copied: "chat-copy-code-done",
};
pub(super) const DIFF: RichCopy = RichCopy {
    name: "chat-copy-diff",
    label: "chat-copy-diff-label",
    copied: "chat-copy-diff-done",
};
pub(super) const LATEX: RichCopy = RichCopy {
    name: "chat-copy-latex",
    label: "chat-copy-latex-label",
    copied: "chat-copy-latex-done",
};
pub(super) const ORIGINAL: RichCopy = RichCopy {
    name: "chat-copy-original",
    label: "chat-copy-original-label",
    copied: "chat-copy-original-done",
};
// keep-sorted end

impl SheprdWindow {
    /// A copy button by a message, positioned by the selected layout.
    pub(super) fn render_chat_copy(
        &self,
        pane: &str,
        id: u64,
        text: &str,
        placement: CopyPlacement,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let compact = placement == CopyPlacement::Floating;
        let words: std::sync::Arc<str> = text.into();
        let button = self
            .chat_copy_button(
                ("chat-copy", id),
                format!("chat-copy-{id}"),
                words.clone(),
                fl!("chat-copy-message-done"),
                cx,
            )
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(4.))
            .rounded(px(corners::SMALL))
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .path("icons/copy.svg")
                    .size(px(11.))
                    .text_color(rgb(theme.muted)),
            )
            .role(Role::Button)
            .aria_label(fl!("chat-copy"))
            .when(!compact, |b| b.child(fl!("chat-copy")));
        let target = crate::voice::Kind::Message {
            window: cx.entity_id(),
            pane: pane.to_owned().into(),
            entry: id,
        };
        let playing = crate::voice::Session::playing(&target);
        let label = if playing {
            fl!("settings-voice-stop-reading")
        } else {
            fl!("settings-voice-read")
        };
        let read = div()
            .id(("chat-read-aloud", id))
            .role(Role::Button)
            .aria_label(label.clone())
            .cursor_pointer()
            .text_color(rgb(theme.muted))
            .text_size(px((self.chat_text_size() - 2.).max(10.)))
            .px(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .when(!compact, |b| b.child(label))
            .when(compact && playing, |b| {
                b.child(div().size(px(9.)).rounded(px(1.)).bg(rgb(theme.muted)))
            })
            .when(compact && !playing, |b| {
                b.child(
                    svg()
                        .path("icons/play.svg")
                        .size(px(12.))
                        .text_color(rgb(theme.muted)),
                )
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.stop_propagation();
                let words = words.clone();
                cx.notify();
                let Some(session) = crate::voice::Session::begin(target.clone()) else {
                    return;
                };
                let job = cx.background_executor().spawn(async move {
                    let result = crate::voice::read_message(&words, &session);
                    if session.cancelled() { Ok(()) } else { result }
                });
                cx.spawn(async move |this, cx| {
                    let result = job.await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(error) = result {
                            this.show_flash(Flash::warning(error.to_string()), cx);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }));
        let actions = div()
            .id(("chat-message-actions", id))
            .flex()
            .items_center()
            .when(!playing, |row| row.opacity(0.))
            .group_hover("chat-agent-message", |row| row.opacity(1.))
            .child(button)
            .child(read);
        layout_for(self.config.chat.layout)
            .copy(actions, placement, &self.chat_look())
            .when(placement == CopyPlacement::Footer, |copy| {
                copy.debug_selector(move || format!("chat-copy-footer-{id}"))
            })
    }

    /// A rich block's copy button, which stays visible.
    pub(super) fn rich_copy(
        &self,
        copy: RichCopy,
        source: impl AsRef<str> + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let copied = crate::i18n::message(copy.copied, &[]);
        self.rich_button(copy.name, crate::i18n::message(copy.label, &[]))
            .on_click(copy_on_click(source, copied, cx))
    }

    /// A button in a rich block's header, such as a code block's Copy or
    /// Wrap; the caller says what a click does.
    pub(super) fn rich_button(&self, name: &'static str, label: String) -> Stateful<Div> {
        let look = self.chat_look();
        let backdrop = layout_for(self.config.chat.layout).backdrop(&look);
        crate::chat::code_block::button(
            &self.theme,
            (self.chat_text_size() - 2.).max(10.),
            backdrop,
            name,
            label,
        )
    }

    /// What every copy button shares: it copies `text`, says `copied` in a
    /// flash, and keeps its press from starting a text selection.
    pub(super) fn chat_copy_button(
        &self,
        id: impl Into<ElementId>,
        selector: String,
        text: impl AsRef<str> + 'static,
        copied: String,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id)
            .debug_selector(move || selector)
            .cursor_pointer()
            .text_size(px((self.chat_text_size() - 2.).max(10.)))
            .hover(|button| button.bg(rgb(theme.active)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(copy_on_click(text, copied, cx))
    }
}

/// Copies `text` and says `copied` in a flash.
fn copy_on_click(
    text: impl AsRef<str> + 'static,
    copied: String,
    cx: &mut Context<SheprdWindow>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _: &ClickEvent, _, cx| {
        cx.stop_propagation();
        cx.write_to_clipboard(ClipboardItem::new_string(text.as_ref().to_owned()));
        this.show_flash(Flash::success(copied.clone()), cx);
    })
}

#[cfg(test)]
mod tests;
