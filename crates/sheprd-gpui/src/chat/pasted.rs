//! A picture, or a long text, pasted into the composer. The program attaches
//! files, so what was pasted is saved to a private file of its own, named by
//! what it holds, and attached as a dropped file would be.

mod error;

pub(crate) use error::Error;

use crate::{
    SheprdWindow,
    i18n::{Localize, fl},
    window::image_source::{PreparedImage, Source},
};
use gpui::{AppContext as _, Context, Image, Window};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

/// Whether pasted `text` meets `[chat]`'s rule for attaching as a `.txt`
/// file rather than filling the composer.
pub(crate) fn is_long_text(text: &str, chat: &crate::config::chat::ChatConfig) -> bool {
    chat.long_paste().is_some_and(|rule| rule.matches(text))
}

/// What was pasted, to be saved as a file.
enum Paste {
    Image(Image),
    Text(String),
}

impl SheprdWindow {
    /// Saves `image` off the UI thread, then adds the file to the next
    /// prompt of the chat over `pane_id`.
    pub(super) fn attach_chat_image(
        &mut self,
        pane_id: &str,
        image: Image,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.attach_pasted(pane_id, Paste::Image(image), window, cx);
    }

    /// Saves `text` to a `.txt` file off the UI thread, then adds the file
    /// to the next prompt of the chat over `pane_id`.
    pub(super) fn attach_chat_text(
        &mut self,
        pane_id: &str,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.attach_pasted(pane_id, Paste::Text(text), window, cx);
    }

    /// Until it is saved the paste is pending, so a prompt sent meanwhile
    /// waits for it, and it joins only the conversation it was pasted into.
    fn attach_pasted(
        &mut self,
        pane_id: &str,
        paste: Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        pane.uploading += 1;
        let target = pane.target.clone();
        let cursor = pane.session.since();
        let is_image = matches!(paste, Paste::Image(_));
        let save = cx.background_spawn(async move { save(paste) });
        let pane_id = pane_id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let saved = save.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(pane) = this
                    .chats
                    .pane_mut(&pane_id)
                    .filter(|pane| pane.target == target)
                else {
                    return;
                };
                pane.uploading = pane.uploading.saturating_sub(1);
                if !pane.is_current(&target, cursor) {
                    // Another conversation took the pane; the paste was
                    // for the one that left.
                    this.redraw_terminal(cx);
                    return;
                }
                match saved {
                    Ok(path) => this.attach_chat_files(&pane_id, vec![path], window, cx),
                    Err(error) => {
                        let error = error.localize();
                        pane.error = Some(if is_image {
                            fl!("chat-image-not-attached", error = error)
                        } else {
                            fl!("chat-text-not-attached", error = error)
                        });
                        this.redraw_terminal(cx);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

/// Blocking: for a picture decodes and may transcode; writes the file.
fn save(paste: Paste) -> crate::Result<PathBuf> {
    let (kind, prefix, extension, bytes) = match paste {
        Paste::Image(image) => {
            let PreparedImage {
                bytes, extension, ..
            } = Source::Clipboard(image).prepare()?;
            ("images", "", extension, bytes)
        }
        Paste::Text(text) => ("pastes", "pasted-", "txt", text.into_bytes()),
    };
    let root = super::page::private_folder(kind)
        .ok_or(Error::Folder)
        .map_err(super::Error::from)?;
    Ok(write(&root, prefix, extension, &bytes).map_err(super::Error::from)?)
}

fn write(root: &Path, prefix: &str, extension: &str, bytes: &[u8]) -> Result<PathBuf, Error> {
    super::page::private_dir(root).map_err(Error::Write)?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let file = root.join(format!("{prefix}{:016x}.{extension}", hasher.finish()));
    std::fs::write(&file, bytes).map_err(Error::Write)?;
    Ok(file)
}

#[cfg(test)]
mod tests;
