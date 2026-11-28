//! Long user messages in the desktop transcript. Paste boundaries aren't part
//! of the wire format. Project the stored prompt, including in older chats,
//! and materialize a local text file only when the reader opens it.

use super::selection::{Cell, Region};
use crate::{
    SheprdWindow,
    config::{chat::LongText, corners},
    i18n::fl,
    notifications::safe_text,
    window::Flash,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use gpui::{prelude::*, *};
use sha2::{Digest, Sha256};
use sheprd_chat_protocol::{EntryId, assist::typed};
use std::{
    cell::OnceCell,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const FILENAME: &str = "Message.txt"; // i18n: data

#[derive(Debug)]
pub(crate) struct UserTextFile {
    pub(super) attachments: String,
    pub(super) preview: String,
    text: Arc<str>,
    focus: OnceCell<FocusHandle>,
}

impl UserTextFile {
    /// The prompt `shown` as a file, when what was typed of it meets `rule`.
    pub(super) fn from_prompt(shown: &str, files: usize, rule: LongText) -> Option<Self> {
        let text = typed(shown, files);
        if !rule.matches(text) {
            return None;
        }
        let first = text.trim_start().lines().next().unwrap_or_default();
        Some(Self {
            attachments: shown[text.len()..].trim_start_matches('\n').into(),
            preview: safe_text(first, 100),
            text: text.into(),
            focus: OnceCell::new(),
        })
    }
}

// Publish a complete file, even on concurrent opens. Reuse the same working
// copy on subsequent clicks so opening it again preserves edits in the editor.
fn write(root: &Path, text: &str) -> io::Result<PathBuf> {
    let folder = root.join(URL_SAFE_NO_PAD.encode(Sha256::digest(text.as_bytes())));
    super::page::private_dir(&folder)?;
    let path = folder.join(FILENAME);
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    file.write_all(text.as_bytes())?;
    match file.persist_noclobber(&path) {
        Ok(_) => {}
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.error),
    }
    Ok(path)
}

impl SheprdWindow {
    pub(in crate::chat) fn render_user_text_file(
        &self,
        pane: &str,
        entry: EntryId,
        file: &UserTextFile,
        cx: &mut Context<Self>,
    ) -> Div {
        let text = file.text.clone();
        let button_pane = pane.to_owned();
        let ring = rgb(self.theme.primary());
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(5.))
            .when(!file.preview.is_empty(), |row| {
                row.child(self.render_chat_text(
                    pane,
                    entry,
                    Region::Body(0),
                    Cell::default(),
                    file.preview.clone(),
                    cx,
                ))
            })
            .child(
                div()
                    .id(("chat-open-message", entry.0))
                    .debug_selector(|| "chat-open-message".into())
                    .track_focus(file.focus.get_or_init(|| cx.focus_handle()))
                    .tab_index(0)
                    .focus_visible(move |style| style.border_color(ring))
                    .role(Role::Button)
                    .aria_label(fl!("chat-open-message-text"))
                    .self_start()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(self.theme.surface))
                    .border_1()
                    .border_color(rgb(self.theme.active))
                    .hover(|el| el.bg(rgb(self.theme.active)))
                    .cursor_pointer()
                    .child(crate::icons::icon("icons/file.svg", 14., self.theme.muted))
                    .child(FILENAME)
                    .child(fl!("chat-file-open"))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        // GPUI activates a focused click listener on Enter/Space key-up.
                        // Keep its key-down away from the composer and terminal.
                        cx.stop_propagation();
                        if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified()
                        {
                            this.focus_chat(&button_pane, window, cx);
                            window.prevent_default();
                        } else if Self::message_tab(event) {
                            this.step_message_file(
                                &button_pane,
                                Some(entry),
                                event.keystroke.modifiers.shift,
                                window,
                                cx,
                            );
                            window.prevent_default();
                        }
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.open_user_text_file(text.clone(), cx);
                    })),
            )
            .when(!file.attachments.is_empty(), |row| {
                row.child(self.render_chat_text(
                    pane,
                    entry,
                    Region::Body(1),
                    Cell::default(),
                    file.attachments.clone(),
                    cx,
                ))
            })
    }

    pub(super) fn message_tab(event: &KeyDownEvent) -> bool {
        let key = &event.keystroke;
        key.key == "tab" && (!key.modifiers.modified() || key.modifiers == Modifiers::shift())
    }

    /// Traverse the unfolded file buttons in transcript order, then return to the input.
    pub(super) fn step_message_file(
        &mut self,
        pane_id: &str,
        current: Option<EntryId>,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.chats.pane(pane_id) else {
            return false;
        };
        let entries = &pane.session.transcript().entries;
        let mut range = 0..entries.len();
        if let Some(index) = current.and_then(|id| entries.iter().position(|entry| entry.id == id))
        {
            if reverse {
                range.end = index;
            } else {
                range.start = index + 1;
            }
        }
        if let Some(fold) = pane.earlier().filter(|fold| !fold.open) {
            range.start = range.start.max(fold.start);
        }
        let mut stops = range.filter_map(|index| {
            let body = pane.prepared.body(entries[index].id)?;
            let super::prepared::Body::UserTextFile(file) = &*body else {
                return None;
            };
            Some((index, file.focus.get_or_init(|| cx.focus_handle()).clone()))
        });
        let next = if reverse {
            stops.next_back()
        } else {
            stops.next()
        };
        if let Some((index, next)) = next {
            let viewport = pane.list.viewport_bounds();
            if !pane
                .list
                .bounds_for_item(index)
                .is_some_and(|row| row.top() >= viewport.top() && row.bottom() <= viewport.bottom())
            {
                self.go_to_chat_prompt(pane_id, index, cx);
            }
            window.focus(&next, cx);
        } else if current.is_some() {
            self.focus_chat(pane_id, window, cx);
        } else {
            return false;
        }
        true
    }

    fn open_user_text_file(&self, text: Arc<str>, cx: &mut Context<Self>) {
        let saved = cx.background_spawn(async move {
            let root = super::page::private_folder("messages")
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            write(&root, &text)
        });
        cx.spawn(async move |this, cx| {
            let result = saved.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(path) => this.open_in_system_app(path, cx),
                Err(error) => this.show_flash(
                    Flash::warning(fl!("chat-message-file-error", error = error.to_string())),
                    cx,
                ),
            });
        })
        .detach();
    }
}
