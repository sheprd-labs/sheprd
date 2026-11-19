//! The right-hand Changed panel: the focused checkout's files, always beside
//! the terminals rather than only inside a review tab.

use crate::{
    SheprdWindow,
    pull_request::Input,
    review::diff::{self, Loaded, Scope},
};
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

/// Git's focused status refreshes every five seconds. Re-list files at the
/// same cadence because equal line totals do not prove the paths stayed put.
const FILES_REFRESH: Duration = Duration::from_secs(5);

fn refresh_due(last_started: Option<Instant>, now: Instant) -> bool {
    last_started.is_none_or(|last| now.duration_since(last) >= FILES_REFRESH)
}

#[derive(Default)]
pub(crate) struct ChangesPanel {
    request: u64,
    checkout: Option<Input>,
    seen: Option<crate::git::Status>,
    last_started: Option<Instant>,
    loading: bool,
    refresh_pending: bool,
    files_scroll: UniformListScrollHandle,
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Loading,
    Loaded(Loaded),
    Failed(String),
}

impl SheprdWindow {
    /// Lists the focused checkout's uncommitted files in the background.
    /// Git status triggers prompt updates; the timer also catches path and
    /// mode changes that leave its line totals unchanged.
    pub(crate) fn follow_changes_panel(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.config.show_changes_panel {
            return self.changes_take();
        }
        let tracked = self.git.tracked().cloned();
        let status = self.git.status();
        let Some(checkout) = tracked else {
            return self.changes_take();
        };

        let checkout_changed = self.mods.changes.checkout.as_ref() != Some(&checkout);
        if checkout_changed {
            self.mods.changes.request = self.mods.changes.request.wrapping_add(1);
            self.mods.changes.checkout = Some(checkout.clone());
            self.mods.changes.seen = None;
            self.mods.changes.refresh_pending = true;
            self.mods.changes.state = State::Loading;
        }
        if let Some(status) = status
            && self.mods.changes.seen != Some(status)
        {
            self.mods.changes.seen = Some(status);
            self.mods.changes.refresh_pending = true;
        }
        let due = refresh_due(self.mods.changes.last_started, Instant::now());
        if due {
            self.mods.changes.refresh_pending = true;
        }
        if self.mods.changes.loading || !self.mods.changes.refresh_pending {
            return checkout_changed;
        }

        self.mods.changes.request = self.mods.changes.request.wrapping_add(1);
        self.mods.changes.loading = true;
        self.mods.changes.refresh_pending = false;
        self.mods.changes.last_started = Some(Instant::now());
        let request = self.mods.changes.request;
        let loading = cx
            .background_executor()
            .spawn(async move { diff::load(&checkout, Scope::Uncommitted, None, None, false) });
        cx.spawn(async move |this, cx| {
            let result = loading.await;
            this.update(cx, |this, cx| {
                this.mods.changes.loading = false;
                if this.mods.changes.request == request {
                    this.mods.changes.state = match result {
                        Ok(loaded) => State::Loaded(loaded),
                        Err(error) => State::Failed(error.to_string()),
                    };
                }
                if !this.follow_changes_panel(cx) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn changes_take(&mut self) -> bool {
        if matches!(self.mods.changes.state, State::Idle) && self.mods.changes.checkout.is_none() {
            return false;
        }
        let request = self.mods.changes.request.wrapping_add(1);
        // Keep the in-flight bit until its worker returns. If the user
        // switches projects or toggles the panel back on meanwhile, the next
        // scan waits for that worker before starting another one.
        let loading = self.mods.changes.loading;
        self.mods.changes = ChangesPanel {
            request,
            loading,
            ..ChangesPanel::default()
        };
        true
    }

    pub(crate) fn render_changes_panel(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let size = self.config.ui.size;
        let small = (size * 0.88).round();
        let (count, summary) = match &self.mods.changes.state {
            State::Idle => (0, "No repository".to_owned()), // i18n: data
            State::Loading => (0, "Checking".to_owned()),   // i18n: data
            State::Failed(error) => (0, format!("Could not read changes: {error}")),
            State::Loaded(loaded) if loaded.diff.files.is_empty() => {
                (0, "No changes".to_owned()) // i18n: data
            }
            State::Loaded(loaded) => {
                let count = loaded.diff.files.len();
                (count, format!("{count} files")) // i18n: data
            }
        };
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(6.))
            .text_color(rgb(theme.foreground))
            .font_weight(FontWeight::SEMIBOLD)
            .child("Changed")
            .child(
                div()
                    .text_size(px(small))
                    .text_color(rgb(theme.muted))
                    .min_w_0()
                    .truncate()
                    .child(summary),
            );
        let files = uniform_list(
            "changes-file-list",
            count,
            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                this.changes_file_rows(range, cx)
            }),
        )
        .track_scroll(&self.mods.changes.files_scroll)
        .flex_1()
        .min_h_0();

        div()
            .debug_selector(|| "changes-panel".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(5.))
            .px(px(12.))
            .py(px(10.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(header)
            .child(files)
    }

    fn changes_file_rows(
        &mut self,
        range: std::ops::Range<usize>,
        _cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = &self.theme;
        let size = self.config.ui.size;
        let line_height = (size * 1.7).max(22.);
        let State::Loaded(loaded) = &self.mods.changes.state else {
            return Vec::new();
        };
        range
            .filter_map(|index| {
                let file = loaded.diff.files.get(index)?;
                let status_color = match file.status {
                    diff::Status::Added | diff::Status::Untracked => theme.ink(theme.palette[2]),
                    diff::Status::Deleted => theme.ink(theme.palette[1]),
                    diff::Status::Renamed => theme.ink(theme.palette[4]),
                    diff::Status::Modified => theme.ink(theme.palette[3]),
                };
                let additions = file.added.map_or("—".to_owned(), |n| format!("+{n}"));
                let deletions = file.removed.map_or("—".to_owned(), |n| format!("−{n}"));
                Some(
                    div()
                        .id(("changes-file", index))
                        .debug_selector(move || format!("changes-file-{index}"))
                        .w_full()
                        .h(px(line_height))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_size(px((size * 0.88).round()))
                        .child(
                            div()
                                .flex_none()
                                .w(px(12.))
                                .text_color(rgb(status_color))
                                .child(file.status.letter()),
                        )
                        .child(div().min_w_0().flex_1().truncate().child(file.path.clone()))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.ink(theme.palette[2])))
                                .child(additions),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.ink(theme.palette[1])))
                                .child(deletions),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn file_list_refreshes_even_when_git_totals_stay_the_same() {
        let now = Instant::now();
        assert!(refresh_due(None, now));
        assert!(!refresh_due(Some(now - Duration::from_secs(1)), now));
        assert!(refresh_due(Some(now - FILES_REFRESH), now));
    }
}
