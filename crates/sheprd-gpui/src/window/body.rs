//! The window's body: the sidebar, and beside it the box the tabs fill, with
//! the notices and the status bar under it.
//!
//! A Tailscale check dims that box and centres its prompt in it, rather than
//! dimming the window: the sidebar, the tab strip, and the status bar carry the
//! same check and stay legible beside it while the user opens the address.

use super::SheprdWindow;
use crate::direction::Logical as _;
use gpui::{AnyElement, Context, Window, div, prelude::*};

impl SheprdWindow {
    /// The row under the title bar. `sidebar` is the column a layout places,
    /// with its width; `merged` says the tab strips sit in the title bar, so
    /// the sidebar is drawn by [`Self::sidebar_column`] instead.
    pub(super) fn render_body(
        &mut self,
        sidebar: Option<(AnyElement, f32)>,
        content: AnyElement,
        merged: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sidebar = sidebar.map(|(sidebar, width)| {
            let column = match merged {
                true => self.sidebar_column(sidebar, window, cx).into_any_element(),
                false => sidebar,
            };
            self.sliding_sidebar(column, width)
        });
        div()
            .debug_selector(|| "window-body".into())
            // The sidebar leads, on the right in a right-to-left language; the
            // terminal area beside it never mirrors.
            .row()
            .flex_1()
            .min_h_0()
            .relative()
            .children(sidebar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .relative()
                    // Whatever the body holds, a check dims this box and hangs
                    // its prompt in the middle of it.
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(content)
                            .children(self.render_tailscale_prompt()),
                    )
                    .children(self.render_notices(cx))
                    .when(!self.hosted, |column| {
                        column.children(self.zen_status_bar(cx))
                    }),
            )
            .into_any_element()
    }
}
