//! What a VS Code tab shows until its page is there: why it is empty, the
//! license to accept, or that VS Code is starting.
use super::{code::Reach, code_host::CodeHost};
use crate::i18n::{Localize, fl};
use crate::{
    SheprdWindow,
    code_server::{Launcher, Startup, VS_CODE, product_names},
};
use gpui::{prelude::*, *};

impl SheprdWindow {
    /// Why the VS Code tab's page is not there yet: `failure` says why it
    /// could not be created.
    pub(super) fn render_code_status(
        &self,
        failure: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = self.code_host(cx);
        let startup = self.code_startup(&host, cx);
        // Servers on SSH hosts are VS Code's; only this computer's may be
        // another editor found in its place.
        let (editor, editors) = match &host {
            CodeHost::Local => (
                Launcher::product(cx).map_or(VS_CODE.name(), |product| product.name()),
                product_names(),
            ),
            CodeHost::Ssh(_) => (VS_CODE.name(), VS_CODE.name().to_owned()),
        };
        let text: SharedString = if !super::EMBEDDED {
            fl!("browser-vscode-unsupported").into()
        } else if let Some(message) = failure {
            fl!("browser-page-failed", error = message.to_string()).into()
        } else {
            match startup {
                Startup::Address if self.config.code.url.is_none() => {
                    fl!("browser-vscode-no-server").into()
                }
                Startup::Finding => fl!("browser-vscode-finding", editors = editors).into(),
                Startup::Missing => fl!("browser-vscode-missing", editors = editors).into(),
                Startup::Consent => return self.render_code_consent(cx),
                Startup::Idle | Startup::Starting { .. } => {
                    fl!("browser-vscode-starting", editor = editor).into()
                }
                Startup::Failed { error, .. } => {
                    return self.render_code_unreachable(error.localize().into());
                }
                Startup::Address | Startup::Ready { .. } => match self
                    .code_server(&host)
                    .map_or(&Reach::Unknown, |server| &server.state)
                {
                    Reach::Failed { message, .. } => {
                        return self.render_code_unreachable(message.clone());
                    }
                    Reach::Ready { server, .. } => {
                        fl!("browser-vscode-loading", commit = server.short_commit()).into()
                    }
                    Reach::Unknown | Reach::Asking => fl!("browser-vscode-connecting").into(),
                },
            }
        };
        centered(
            div()
                .debug_selector(|| "code-placeholder".into())
                .text_color(rgb(self.theme.muted))
                .child(text),
        )
    }

    /// Why the server did not answer, and that it is asked again.
    fn render_code_unreachable(&self, message: SharedString) -> AnyElement {
        let theme = &self.theme;
        centered(
            div()
                .debug_selector(|| "code-unreachable".into())
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .text_center()
                .child(
                    svg()
                        .path("icons/error.svg")
                        .size(px(32.))
                        .text_color(rgb(theme.ink(theme.palette[1]))),
                )
                .child(
                    div()
                        .text_color(rgb(theme.foreground))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(fl!("browser-vscode-unreachable")),
                )
                .child(div().text_color(rgb(theme.muted)).child(message))
                .child(
                    div()
                        .text_color(rgb(theme.muted))
                        .text_sm()
                        .child(fl!("browser-vscode-retrying")),
                ),
        )
    }

    /// Whether VS Code is offered, in each group's "…" menu and as Open
    /// VS Code: once a server is set, or the app starts VS Code, in a build
    /// that can show pages.
    pub(crate) fn code_offered(&self, cx: &App) -> bool {
        super::EMBEDDED
            && self
                .code_startup(&self.code_host(cx), cx)
                .offered(&self.config.code)
    }
}

/// `content` in the middle of the tab, both ways.
pub(super) fn centered(content: impl IntoElement) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .child(content)
        .into_any_element()
}
