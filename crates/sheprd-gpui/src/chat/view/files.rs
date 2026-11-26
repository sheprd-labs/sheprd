//! File cards reuse the explicit link opener, including its session fence and
//! background checks. Eligible local PDFs/videos request bounded thumbnails;
//! external applications still open only after an explicit click.

use crate::chat::{
    view::rich::links::Links,
    visual::{Shown, Source, VisualId, Visuals, thumbnail},
};
use crate::terminal::{PaneLink, RowLink, RowTarget, file_target};
use crate::window::FileLink;
use crate::{SheprdWindow, config::corners, i18n::fl};
use gpui::{prelude::*, *};

/// Named after the file manager it opens where the platform has one name.
fn reveal_label() -> String {
    if cfg!(target_os = "macos") {
        fl!("chat-file-reveal")
    } else {
        fl!("chat-file-reveal-folder")
    }
}

impl SheprdWindow {
    pub(super) fn render_chat_file(
        &self,
        pane_id: &str,
        (id, destination): (VisualId, &str),
        content: AnyElement,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let local = self
            .chats
            .pane(pane_id)
            .is_some_and(|pane| pane.target.is_local());
        let preview = self.chat_file_thumbnail(pane_id, destination);
        if let Some(file) = preview.as_ref() {
            visuals.request_visible(id, Source::Thumbnail(file.clone()));
        }
        let preview_links = Links::new(self, pane_id, cx);
        let preview_destination = destination.to_owned();
        let thumbnail = div()
            .id("chat-file-preview")
            .debug_selector(|| "chat-file-preview".into())
            .when(local, |preview| {
                preview
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        preview_links.open_attachment(&preview_destination, cx);
                    })
            })
            .flex_none()
            .w(px(96.))
            .h(px(68.))
            .flex()
            .items_center()
            .justify_center()
            .child(match (preview, visuals.shown(id)) {
                (Some(_), Shown::Ready(drawn)) => {
                    let scale = (96. / drawn.width).min(68. / drawn.height);
                    img(drawn.texture.clone())
                        .debug_selector(|| "chat-file-thumbnail".into())
                        .w(px(drawn.width * scale))
                        .h(px(drawn.height * scale))
                        .rounded(px(corners::CONTROL))
                        .overflow_hidden()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element()
                }
                _ => crate::icons::icon("icons/file.svg", 32., self.theme.muted),
            });
        let links = Links::new(self, pane_id, cx);
        let destination = destination.to_owned();
        div()
            .debug_selector(|| "chat-file-card".into())
            .min_w_0()
            .w(px(440.))
            .max_w_full()
            .self_start()
            .p(px(12.))
            .rounded(px(corners::PANEL + corners::SMALL))
            .border_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.surface))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(thumbnail)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(content)
                    .child(if local {
                        let reveal_links = links.clone();
                        let reveal_destination = destination.clone();
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                self.chat_file_button("chat-file-open", fl!("chat-file-open"))
                                    .child(crate::icons::icon(
                                        "icons/chevron-right.svg",
                                        12.,
                                        self.theme.subtext(),
                                    ))
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        links.open_attachment(&destination, cx);
                                    }),
                            )
                            .child(
                                self.chat_file_button("chat-file-reveal", reveal_label())
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        reveal_links.reveal_attachment(&reveal_destination, cx);
                                    }),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .debug_selector(|| "chat-file-open".into())
                            .text_size(self.chat_small_text())
                            .text_color(rgb(self.theme.subtext()))
                            .child(fl!("chat-file-on-host"))
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    /// A small pill on a local file card.
    fn chat_file_button(&self, id: &'static str, label: String) -> Stateful<Div> {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .text_size(self.chat_small_text())
            .text_color(rgb(self.theme.subtext()))
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(8.))
            .py(px(3.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(self.theme.active))
            .hover(|button| {
                button.bg(rgb(crate::config::mix(
                    self.theme.active,
                    self.theme.foreground,
                    8,
                )))
            })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(label)
    }

    /// No I/O here. The worker resolves symlinks and checks the final file.
    pub(in crate::chat) fn chat_file_thumbnail(
        &self,
        pane_id: &str,
        destination: &str,
    ) -> Option<FileLink> {
        if !cfg!(target_os = "macos")
            || self.selected_is_remote()
            || !self
                .chats
                .pane(pane_id)
                .is_some_and(|pane| pane.target.is_local())
        {
            return None;
        }
        let target = file_target(destination)?;
        let RowTarget::Path {
            path, line: None, ..
        } = &target
        else {
            return None;
        };
        if !thumbnail::supported(std::path::Path::new(path)) {
            return None;
        }
        self.file_link(PaneLink {
            pane_id: pane_id.to_owned(),
            link: RowLink {
                target,
                row: 0,
                columns: 0..0,
                continued: Vec::new(),
            },
        })
    }
}
