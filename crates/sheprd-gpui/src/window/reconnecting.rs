//! What the terminal area shows while the selected endpoint reconnects after a
//! drop: the last picture dimmed, and a card naming the host, why it dropped,
//! and a way to retry now. The endpoint keeps retrying on its own backoff and
//! the window stays on it; nothing typed meanwhile is sent.
//!
//! A Tailscale check is not drawn here: a refusal during the handshake leaves
//! no outage to hang a card on, and the check needs the same prompt whether or
//! not a terminal exists, so the prompt dims the box the tabs fill and centres
//! in it, rather than the window around it.
// i18n: migrated
use super::SheprdWindow;
use crate::{i18n::fl, notifications::safe_text};
use gpui::{prelude::*, *};

const MAX_WIDTH: f32 = 420.;
const MAX_REASON: usize = 240;
/// How much of the stale picture shows through.
const DIM_ALPHA: u32 = 0xa0;

impl SheprdWindow {
    /// Whether this host is waiting on a Tailscale check, by the one rule every
    /// surface reads: the endpoint's own answer.
    fn awaiting_tailscale(&self) -> bool {
        self.endpoints[self.selected_endpoint]
            .tailscale_check()
            .is_some()
    }

    /// Whether the card shows. A replacement connection's snapshot ends the
    /// outage before its first frame, so the card stays over the old picture
    /// until that frame lands.
    pub(crate) fn reconnecting(&self) -> bool {
        self.endpoints[self.selected_endpoint].outage().is_some() || self.presentation.stale()
    }

    pub(super) fn render_reconnecting(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // A pending check has its own prompt, which dims the tabs box and
        // does not depend on there being a terminal to cover: drawing the
        // card as well would put two answers to one question on screen.
        if self.awaiting_tailscale() || !self.reconnecting() {
            return None;
        }
        let endpoint = &self.endpoints[self.selected_endpoint];
        // A device going away drops on purpose: say so, and offer no retry.
        let removing = self.device_removing(endpoint);
        let reason = if removing {
            Some(fl!("window-device-being-removed"))
        } else {
            endpoint.outage().map(str::to_owned)
        };
        let theme = &self.theme;
        let card = div()
            .id("reconnecting")
            .debug_selector(|| "reconnecting".into())
            .min_w_0()
            .max_w(px(MAX_WIDTH))
            .occlude()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(theme.ink(theme.palette[3])))
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.foreground))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(div().font_weight(FontWeight::SEMIBOLD).truncate().child({
                let host = safe_text(&endpoint.label, 80);
                if removing {
                    fl!("window-removing", host = host)
                } else {
                    fl!("window-reconnecting", host = host)
                }
            }))
            .children(reason.filter(|reason| !reason.is_empty()).map(|reason| {
                div()
                    .debug_selector(|| "reconnecting-reason".into())
                    .text_color(rgb(theme.muted))
                    .child(safe_text(&reason, MAX_REASON))
            }))
            .when(!removing, |card| {
                card.child(
                    div().flex().child(
                        div()
                            .id("reconnect-now")
                            .debug_selector(|| "reconnect-now".into())
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .border_1()
                            .border_color(rgb(theme.active))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme.active)))
                            .child(fl!("window-reconnect-now"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.reconnect(cx);
                                cx.notify();
                            })),
                    ),
                )
            });
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p(px(16.))
                .when(self.presentation.stale(), |overlay| {
                    overlay.bg(rgba((theme.background << 8) | DIM_ALPHA))
                })
                .child(card)
                .into_any_element(),
        )
    }

    /// The address a pending check printed, over the box the tabs fill. A
    /// refusal during the handshake leaves no terminal to hang a card on, so
    /// this dims that whole box, whatever the body holds, and centres the
    /// prompt in it: the sidebar, tab strip, and status bar stay as they are.
    ///
    /// The sentence comes from the locale files whole and stays whole: the
    /// address has a line of its own below it, and a surface that cut it out of
    /// a translated sentence would be cutting words the reader still needs.
    pub(super) fn render_tailscale_prompt(&self) -> Option<AnyElement> {
        let endpoint = &self.endpoints[self.selected_endpoint];
        let address = endpoint.tailscale_check()?.url()?;
        let open = address.to_owned();
        let theme = &self.theme;
        Some(
            div()
                .absolute()
                .inset_0()
                .debug_selector(|| "tailscale-dim".into())
                // The failure text left un-dimmed under the prompt reads as a
                // second, half-hidden message. It only dims, as a dialog's
                // backdrop does: what lies under it keeps its clicks.
                .bg(rgba((theme.background << 8) | DIM_ALPHA))
                .flex()
                .items_center()
                .justify_center()
                .p(px(16.))
                .child(
                    div()
                        .id("tailscale-prompt")
                        .debug_selector(|| "tailscale-prompt".into())
                        .min_w_0()
                        .max_w(px(MAX_WIDTH))
                        .occlude()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .p(px(12.))
                        .rounded(px(crate::config::corners::PANEL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(theme.foreground))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(fl!(
                                    "window-tailscale-verify",
                                    host = safe_text(&endpoint.label, 80)
                                )),
                        )
                        .child(
                            div()
                                .text_color(rgb(theme.muted))
                                .child(fl!("window-tailscale-check")),
                        )
                        .child(
                            crate::tailscale::address_row(theme, address)
                                .text_color(rgb(theme.ink(theme.palette[3]))),
                        )
                        .child(
                            div().flex().child(
                                div()
                                    .id("tailscale-open")
                                    .debug_selector(|| "tailscale-open".into())
                                    .px(px(10.))
                                    .py(px(4.))
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .border_1()
                                    .border_color(rgb(theme.active))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme.active)))
                                    .child(fl!("window-open-browser"))
                                    .on_click(move |_, _, cx| {
                                        crate::tailscale::open_and_copy(&open, cx);
                                    }),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests;
