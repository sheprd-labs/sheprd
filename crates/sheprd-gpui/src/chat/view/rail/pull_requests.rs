//! The rail's Pull requests section: each pull request the agent worked on
//! in this chat, newest first, colored as its badge is when a lookup of the
//! app's knows it. A click opens it in the Pull requests section when that
//! lists it, else its page on GitHub.

use super::{Heading, LINE, ROWS, chat_section_rows};
use crate::{
    SheprdWindow,
    chat::{chats::ChatPane, rail_folds::RailSection},
    config::corners,
    i18n::fl,
    notifications::safe_text,
    pull_request::{Tone, tone_color},
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::PullRequest;
use sheprd_control_protocol::ActivityPanel;

/// What a row shows of a pull request beyond its number.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Known {
    pub(super) title: Option<String>,
    pub(super) tone: Option<Tone>,
}

/// The tone a listed item's own state word gives, when it settles one:
/// what the section knows of a pull request no checkout looked up.
pub(super) fn state_tone(state: &str) -> Option<Tone> {
    match state.to_ascii_lowercase().as_str() {
        "merged" => Some(Tone::Merged),
        "closed" => Some(Tone::Attention),
        _ => None,
    }
}

impl SheprdWindow {
    /// What the app's pull request lookups, or else the Pull requests
    /// section, know of `pull`.
    fn chat_pull_request_known(&self, pull: &PullRequest, cx: &App) -> Known {
        if let Some(pr) = self.menu.pr_cache.by_url(&pull.url) {
            return Known {
                title: Some(pr.title.clone()),
                tone: Some(pr.tone()),
            };
        }
        let Some(view) = self.section_view() else {
            return Known::default();
        };
        view.read(cx)
            .pull_request_at(&pull.url)
            .map(|item| Known {
                title: Some(item.title.clone()),
                tone: state_tone(&item.state),
            })
            .unwrap_or_default()
    }

    /// Opens the pull request at `url` in the Pull requests section when it
    /// lists it, else its page.
    fn open_chat_pull_request(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity_panel_listed(ActivityPanel::PullRequests)
            && let Some(view) = self.section_view()
            && view.update(cx, |view, cx| view.open_pull_request_at(url, cx))
        {
            self.show_activity_panel(ActivityPanel::PullRequests, window, cx);
            return;
        }
        cx.open_url(url);
    }

    pub(super) fn render_chat_rail_pull_requests(
        &self,
        chat: &ChatPane,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let pulls = &chat.session.transcript().pull_requests;
        if pulls.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let small = self.chat_small_text();
        let rows = pulls
            .iter()
            .rev()
            .take(ROWS)
            .enumerate()
            .map(|(index, pull)| {
                let known = self.chat_pull_request_known(pull, cx);
                let color = known
                    .tone
                    .map_or(theme.subtext(), |tone| tone_color(tone, theme));
                let url = pull.url.clone();
                let label = known.title.unwrap_or_else(|| pull.repo.clone());
                div()
                    .id(("chat-rail-pull-request", index))
                    .debug_selector(move || format!("chat-rail-pull-request-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .mx(px(-4.))
                    .px(px(4.))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.active)))
                    .text_size(small)
                    .child(
                        svg()
                            .flex_none()
                            .path("icons/pull-request.svg")
                            .size(px(11.))
                            .text_color(rgb(color)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(color))
                            .child(format!("#{}", pull.number)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(safe_text(&label, 200)),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_chat_pull_request(&url, window, cx);
                    }))
            });
        let body = div()
            .debug_selector(|| "chat-pull-requests".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(rows.collect::<Vec<_>>())
            .children(self.chat_rail_more(pulls.len()));
        Some(self.render_chat_rail_section(
            chat,
            RailSection::PullRequests,
            busy,
            Heading {
                full: chat_section_rows(pulls.len()) * LINE,
                step: None,
                summary: pulls.len().to_string(),
                title: fl!("chat-rail-pull-requests"),
            },
            body,
            cx,
        ))
    }
}
