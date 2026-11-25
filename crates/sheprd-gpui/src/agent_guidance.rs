//! Per-window agent guidance, shared by the main view and its Settings window.
// i18n: user-supplied guidance text; the dismiss label comes from settings.ftl.
use crate::{config::Theme, direction::Logical, i18n::fl, motion};
use gpui::{prelude::*, *};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) text: String,
    pub(crate) target: Option<&'static str>,
    motion: motion::Toggle,
}

impl State {
    pub(crate) fn shown(&self) -> f32 {
        if cfg!(test) {
            return if self.motion.is_open() { 1. } else { 0. };
        }
        self.motion.shown(motion::now())
    }

    pub(crate) fn update(
        &mut self,
        text: Option<String>,
        target: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = text {
            self.text = text;
        }
        if let Some(target) = target {
            self.target = Some(target);
        }
        self.motion.set(true, motion::now(), false);
        cx.notify();
    }

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.target = None;
        self.motion.set(false, motion::now(), false);
        cx.notify();
    }
}

pub(crate) fn banner(
    state: &Entity<State>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Option<impl IntoElement> {
    let guide = state.read(cx);
    let now = motion::now();
    let shown = guide.shown();
    if !guide.motion.is_open() && shown == 0. {
        return None;
    }
    if guide.motion.moving(now) {
        window.request_animation_frame();
    }
    let close = state.clone();
    Some(
        div()
            .absolute()
            .top(px(40. - 8. * (1. - shown)))
            .left(px(24.))
            .right(px(24.))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("agent-guidance")
                    .max_w(px(640.))
                    .w_full()
                    .rounded(px(12.))
                    .p(px(12.))
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.primary()))
                    .shadow_md()
                    .opacity(shown)
                    .row()
                    .items_start()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(rgb(theme.foreground))
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .line_clamp(3)
                            .child(guide.text.clone()),
                    )
                    .child(
                        div()
                            .id("dismiss-agent-guidance")
                            .cursor_pointer()
                            .child("×")
                            .role(Role::Button)
                            .aria_label(fl!("settings-agent-guide-dismiss"))
                            .on_click(move |_, _, cx| {
                                close.update(cx, |state, cx| state.clear(cx))
                            }),
                    ),
            ),
    )
}
