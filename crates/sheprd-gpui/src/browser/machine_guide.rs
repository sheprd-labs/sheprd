//! The setup guide docked at the right of a machine's tab
//! ([`crate::machine::guide`]): each step with its mark, the current one
//! open with what to do and what Sheprd waits to see, so the guest stays in
//! view while the user works in it.

use crate::{
    SheprdWindow,
    config::{corners, mix},
    i18n::fl,
    machine::{
        Name, Status,
        guide::{self, Shown, Step},
        status,
    },
};
use gpui::{
    Animation, AnimationExt as _, Context, Div, FontWeight, InteractiveElement, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _,
    pulsating_between, px, rgb, svg,
};
use std::time::Duration;

/// The panel's width: room for a step's instructions beside the guest.
const WIDTH: f32 = 280.;
/// How long a waiting dot takes to pulse once.
const PULSE: Duration = Duration::from_millis(1200);

/// What a step says: its title, what to do, what Sheprd waits for, and what
/// it saw once the step is done.
struct Words {
    title: String,
    detail: String,
    waiting: Option<String>,
    done: String,
}

fn words(step: Step) -> Words {
    match step {
        Step::SignIn => Words {
            title: fl!("machine-guide-sign-in"),
            detail: fl!("machine-guide-sign-in-detail"),
            waiting: None,
            done: fl!("machine-guide-sign-in-done"),
        },
        Step::Install => Words {
            title: fl!("machine-guide-install"),
            detail: fl!("machine-guide-install-detail"),
            waiting: Some(fl!("machine-guide-install-waiting")),
            done: fl!("machine-guide-install-done"),
        },
        Step::Accessibility => Words {
            title: fl!("machine-guide-accessibility"),
            detail: fl!("machine-guide-accessibility-detail"),
            waiting: Some(fl!("machine-guide-accessibility-waiting")),
            done: fl!("machine-guide-accessibility-done"),
        },
        Step::ScreenRecording => Words {
            title: fl!("machine-guide-screen-recording"),
            detail: fl!("machine-guide-screen-recording-detail"),
            waiting: Some(fl!("machine-guide-screen-recording-waiting")),
            done: fl!("machine-guide-screen-recording-done"),
        },
        Step::Save | Step::Ready => Words {
            title: fl!("machine-guide-save"),
            detail: fl!("machine-guide-save-detail"),
            waiting: None,
            done: String::new(),
        },
    }
}

impl SheprdWindow {
    /// The guide beside machine `name`'s tab, while it is open.
    pub(super) fn render_machine_guide(&self, name: &Name, cx: &mut Context<Self>) -> Option<Div> {
        let shown = guide::shown(name)?;
        let theme = &self.theme;
        let close = {
            let name = name.clone();
            div()
                .id("machine-guide-close")
                .debug_selector(|| "machine-guide-close".into())
                .flex_none()
                .p_1()
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.active)))
                .on_click(cx.listener(move |_, _, _, cx| guide::close(&name, cx)))
                .child(
                    svg()
                        .size(px(12.))
                        .path("icons/close.svg")
                        .text_color(rgb(theme.muted)),
                )
        };
        let title = if shown.step == Step::Ready {
            fl!("machine-guide-ready")
        } else {
            fl!("machine-guide-title")
        };
        let steps = Step::LISTED
            .iter()
            .enumerate()
            .map(|(index, step)| self.machine_guide_step(name, &shown, index, *step, cx));
        Some(
            div()
                .debug_selector(|| "machine-guide".into())
                .flex_none()
                .w(px(WIDTH))
                .h_full()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .bg(rgb(theme.background))
                .border_l_1()
                .border_color(rgb(mix(theme.background, theme.foreground, 12)))
                .text_color(rgb(theme.foreground))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(close),
                )
                .children(steps)
                .when(shown.step == Step::Ready, |panel| {
                    let image = shown.saved.as_ref().map_or("", Name::as_str);
                    panel.child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("machine-guide-ready-detail", image = image)),
                    )
                })
                .child(div().flex_1())
                .when(shown.step == Step::Ready, |panel| {
                    let name = name.clone();
                    panel.child(
                        self.machine_guide_button(
                            "machine-guide-done",
                            fl!("machine-guide-done"),
                            true,
                        )
                        .on_click(cx.listener(move |_, _, _, cx| guide::close(&name, cx))),
                    )
                }),
        )
    }

    fn machine_guide_step(
        &self,
        name: &Name,
        shown: &Shown,
        index: usize,
        step: Step,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let done = step < shown.step;
        let current = step == shown.step;
        let last = index + 1 == Step::LISTED.len();
        let green = theme.ink(theme.palette[2]);
        let words = words(step);
        let mark = div()
            .flex_none()
            .size(px(18.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .text_xs();
        let mark = if done {
            mark.bg(rgb(green)).child(
                svg()
                    .size(px(10.))
                    .path("icons/check.svg")
                    .text_color(rgb(theme.text_on(green))),
            )
        } else if current {
            mark.bg(rgb(theme.primary()))
                .text_color(rgb(theme.text_on(theme.primary())))
                .child((index + 1).to_string())
        } else {
            mark.border_1()
                .border_color(rgb(theme.muted))
                .text_color(rgb(theme.muted))
                .child((index + 1).to_string())
        };
        let rail = div()
            .flex()
            .flex_col()
            .items_center()
            .child(mark)
            .when(!last, |rail| {
                rail.child(div().w(px(2.)).flex_1().min_h(px(8.)).bg(rgb(if done {
                    green
                } else {
                    theme.surface
                })))
            });
        let text = div()
            .flex_1()
            .min_w_0()
            .pb_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(rgb(if done || current {
                        theme.foreground
                    } else {
                        theme.muted
                    }))
                    .when(current, |title| title.font_weight(FontWeight::SEMIBOLD))
                    .child(words.title),
            )
            .when(done && !words.done.is_empty(), |text| {
                text.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted))
                        .child(words.done),
                )
            })
            .when(current, |text| {
                text.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.subtext()))
                        .child(words.detail),
                )
                .child(self.machine_guide_action(
                    name,
                    shown,
                    step,
                    words.waiting,
                    cx,
                ))
            });
        div().flex().gap_2().child(rail).child(text)
    }

    /// What the current step offers: a button, or what Sheprd waits to see.
    fn machine_guide_action(
        &self,
        name: &Name,
        shown: &Shown,
        step: Step,
        waiting: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let running = status(name) == Status::Running;
        let row = div().flex().flex_col().gap_2().text_sm();
        let note = |text: String| div().text_color(rgb(theme.muted)).child(text);
        match step {
            Step::SignIn => {
                let name = name.clone();
                row.child(
                    self.machine_guide_button(
                        "machine-guide-signed-in",
                        fl!("machine-guide-sign-in-confirm"),
                        true,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| guide::signed_in(&name, cx))),
                )
            }
            Step::Install | Step::Accessibility | Step::ScreenRecording if !running => {
                row.child(note(fl!("machine-guide-not-running")))
            }
            Step::Install | Step::Accessibility | Step::ScreenRecording => {
                row.child(self.machine_guide_waiting(waiting.unwrap_or_default()))
            }
            Step::Save | Step::Ready => {
                let skip = {
                    let name = name.clone();
                    self.machine_guide_button(
                        "machine-guide-skip",
                        fl!("machine-guide-skip"),
                        false,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| guide::close(&name, cx)))
                };
                let stopped = status(name) == Status::Stopped;
                let save = (stopped && !shown.saving).then(|| {
                    let name = name.clone();
                    self.machine_guide_button(
                        "machine-guide-save",
                        fl!("machine-guide-save-button"),
                        true,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| guide::save(&name, cx)))
                });
                row.when(!stopped, |row| {
                    row.child(note(fl!("machine-guide-save-shut-down")))
                })
                .when(shown.saving, |row| {
                    row.child(self.machine_guide_waiting(fl!("machine-guide-saving")))
                })
                .when_some(shown.failure.clone(), |row, failure| {
                    row.child(
                        div()
                            .text_color(rgb(theme.ink(theme.palette[1])))
                            .child(failure),
                    )
                })
                .child(div().flex().gap_2().children(save).child(skip))
            }
        }
    }

    /// A pulsing dot and what Sheprd waits to see.
    fn machine_guide_waiting(&self, text: String) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_color(rgb(theme.subtext()))
            .child(
                div()
                    .flex_none()
                    .size(px(8.))
                    .rounded_full()
                    .bg(rgb(theme.ink(theme.palette[3])))
                    .with_animation(
                        "machine-guide-waiting",
                        Animation::new(crate::motion::duration(PULSE))
                            .repeat()
                            .with_easing(pulsating_between(0.3, 1.)),
                        |dot, delta| dot.opacity(delta),
                    ),
            )
            .child(text)
    }

    fn machine_guide_button(
        &self,
        id: &'static str,
        label: String,
        primary: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let fill = if primary {
            theme.primary()
        } else {
            theme.surface
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .px_3()
            .py_1()
            .rounded(px(corners::CONTROL))
            .bg(rgb(fill))
            .border_1()
            .border_color(rgb(mix(fill, theme.foreground, 12)))
            .text_color(rgb(if primary {
                theme.text_on(fill)
            } else {
                theme.foreground
            }))
            .cursor_pointer()
            .hover(move |style| style.opacity(0.9))
            .child(label)
    }
}
