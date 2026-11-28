//! Prompt file attachments for the chats a window shows: local paths, SSH
//! uploads, the file picker, and removing queued files.

use crate::{
    SheprdWindow,
    chat::{Route, chats::ChatInput},
    i18n::fl,
};
use gpui::{Context, PathPromptOptions, Window};
use sheprd_chat_protocol::MAX_ATTACHMENTS;

impl SheprdWindow {
    /// Adds files to the next prompt of the chat over `pane_id`. A chat on
    /// an SSH host takes copies made there, since this machine's paths name
    /// nothing on that host.
    pub(crate) fn attach_chat_files(
        &mut self,
        pane_id: &str,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .chats
            .pane(pane_id)
            .is_some_and(|pane| pane.input() != ChatInput::Composer)
        {
            return;
        }
        let host = match self.chats.pane(pane_id).map(|pane| &pane.target.route) {
            Some(Route::Remote(target)) => target.clone(),
            Some(Route::Ssh(host)) => sheprd_client::ConnectTarget::Ssh {
                target: host.clone(),
                session: String::new(),
            },
            Some(Route::Local) => return self.add_chat_files(pane_id, paths, window, cx),
            None => return,
        };
        self.upload_chat_files(pane_id, host, paths, window, cx);
    }

    /// Adds paths, as the program will read them, to the next prompt,
    /// skipping ones already there and any past [`MAX_ATTACHMENTS`]. Whether
    /// each exists is checked by the program when the prompt goes out.
    pub(in crate::chat) fn add_chat_files(
        &mut self,
        pane_id: &str,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane.input() != ChatInput::Composer {
            return;
        }
        for path in paths {
            if pane.attachments.len() >= MAX_ATTACHMENTS {
                pane.error = Some(fl!("chat-files-too-many", count = MAX_ATTACHMENTS));
                break;
            }
            if !pane.attachments.contains(&path) {
                pane.attachments.push(path);
            }
        }
        if let Some(input) = pane.composer.clone() {
            let focus = input.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// The system file dialog; the chosen files join the next prompt.
    pub(in crate::chat) fn browse_chat_files(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(fl!("chat-files-attach").into()),
        });
        let pane_id = pane_id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.attach_chat_files(&pane_id, paths, window, cx);
            });
        })
        .detach();
    }

    /// Takes a file back off the next prompt.
    pub(in crate::chat) fn detach_chat_file(
        &mut self,
        pane_id: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.chats.pane_mut(pane_id)
            && index < pane.attachments.len()
        {
            pane.attachments.remove(index);
            self.redraw_terminal(cx);
            cx.notify();
        }
    }
}
