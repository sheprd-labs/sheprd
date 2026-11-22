//! What the delete dialog shows while Herdr removes a checkout. Herdr runs
//! Git off its event loop but answers `worktree.remove` only once Git is
//! done, and the connection sends one API request at a time, so focus and tab
//! changes wait behind a slow removal. A sliding bar and the time waited say
//! the app is not stuck. Git reports no progress, so the bar has no fraction.

use crate::{
    SheprdWindow,
    i18n::fl,
    menu::Removal,
    progress::{Progress, bar},
};
use gpui::{prelude::*, *};

impl SheprdWindow {
    pub(in crate::menu) fn render_removal_progress(&self, removal: &Removal) -> Div {
        let theme = &self.theme;
        let seconds = removal.elapsed().as_secs();
        div()
            .debug_selector(|| "dialog-removing".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(8.))
                    .child(fl!("workspace-dialog-removing"))
                    .child(
                        div()
                            .debug_selector(|| "dialog-removing-elapsed".into())
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("workspace-dialog-removing-elapsed", seconds = seconds)),
                    ),
            )
            .child(bar(
                "dialog-removing-progress",
                Progress::Busy,
                crate::menu::accent(theme).into(),
                rgb(theme.active).into(),
            ))
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("workspace-dialog-removing-note")),
            )
    }
}
