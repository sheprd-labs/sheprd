//! A stacked pull request on its page: a chip in the header saying which
//! layer of its GitHub stack it is, and the popover it opens, which leads with
//! what merging here would do (the verdict, or its merge queue entry), then
//! the layers bottom to top. Each layer opens its own page when it is listed,
//! else GitHub's. Its footer merges the stack or changes its place in the
//! merge queue (`actions`), each after the view's confirmation.

mod actions;
mod inferred;

use super::{Event, OrchestratorView};
use crate::{
    config::{corners, mix},
    i18n::fl,
    pull_request::{
        Blocker, Layer, PullRequest, QueueEntry, QueueState, Stack, State, Tone, Verdict,
        tone_color,
    },
};
use gpui::{prelude::*, *};
use std::borrow::Cow;

pub(super) use actions::{Change, confirm_text};
pub(super) use inferred::infer;

/// Where a stack came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    /// GitHub keeps it: it can be merged and queued as one.
    Github,
    /// Inferred from branch bases: each layer still merges on its own.
    Inferred,
}

/// The stack popover's state on the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Popover {
    /// The pull request whose popover is open.
    pub(super) open: Option<u64>,
    /// Whether "Merge stack" shows the merge methods to pick from.
    pub(super) methods: bool,
}

impl Popover {
    fn toggle(number: u64, current: Self) -> Self {
        Self {
            open: (current.open != Some(number)).then_some(number),
            methods: false,
        }
    }
}

/// The popover's width.
const WIDTH: f32 = 380.;
/// The narrowest a layer's step gets before the strip wraps.
const STEP: f32 = 64.;

impl OrchestratorView {
    /// The header's stack chip, and the popover while it is open; the merge
    /// queue's tag for a pull request in the queue but in no stack.
    pub(super) fn render_stack_chip(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pr = self.open_pull_request()?;
        let look = &self.look;
        let theme = &look.theme;
        let Some((stack, position, source)) = self.open_stack(pr) else {
            let queue = pr.queue?;
            return Some(
                look.tag(
                    fl!("pr-stack-queue-chip", position = queue.position),
                    queue_color(&queue, theme),
                )
                .debug_selector(|| "orchestrator-queue-tag".into())
                .into_any_element(),
            );
        };
        let number = pr.number;
        let open = self.stack.open == Some(number);
        let label = fl!(
            "pr-stack-chip-label",
            position = position,
            size = stack.size
        );
        let chip = div()
            .id("orchestrator-stack-chip")
            .debug_selector(|| "orchestrator-stack-chip".into())
            .role(Role::Button)
            .aria_label(label)
            .aria_expanded(open)
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px(px(6.))
            .py(px(1.))
            .rounded(px(corners::SMALL))
            .border_1()
            .border_color(rgb(if open { theme.primary() } else { theme.active }))
            .bg(rgb(if open {
                theme.primary_wash()
            } else {
                theme.active
            }))
            .cursor_pointer()
            .text_size(look.small())
            .child(look.dot(summary_color(pr, &stack, position, theme), 6.))
            .child(fl!("pr-stack-chip", position = position, size = stack.size))
            .child(look.icon("icons/chevron-down.svg", 10., theme.muted))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.stack = Popover::toggle(number, this.stack);
                cx.notify();
            }));
        if !open {
            return Some(chip.into_any_element());
        }
        // A window-wide shield under the popover: a click anywhere else,
        // the chip included, closes it rather than reaching what is below.
        let size = window.viewport_size();
        let shield = deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(px(0.), px(0.)))
                .child(
                    div()
                        .id("orchestrator-stack-shield")
                        .w(size.width)
                        .h(size.height)
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.stack = Popover::default();
                                cx.notify();
                            }),
                        ),
                ),
        )
        .with_priority(1);
        let popover = deferred(anchored().snap_to_window_with_margin(px(8.)).child(
            crate::motion::pop_in(
                self.stack_popover(pr, &stack, position, source, cx),
                "orchestrator-stack-popover",
            ),
        ))
        .with_priority(2);
        Some(
            div()
                .flex_none()
                .child(chip)
                .child(shield)
                .child(popover)
                .into_any_element(),
        )
    }

    /// The stack the open pull request is a layer of: GitHub's own, else one
    /// inferred from the list's branch bases.
    fn open_stack<'a>(&'a self, pr: &'a PullRequest) -> Option<(Cow<'a, Stack>, u64, Source)> {
        if let Some((stack, position)) = pr.stack() {
            return Some((Cow::Borrowed(stack), position, Source::Github));
        }
        let key = &self.detail.as_ref()?.key;
        let current = self.item(key)?;
        let snapshot = &self.repos[self.repo_of(key)?].snapshot;
        let (stack, position) = infer(&snapshot.items, &snapshot.pr_status, current)?;
        Some((Cow::Owned(stack), position, Source::Inferred))
    }

    fn stack_popover(
        &self,
        pr: &PullRequest,
        stack: &Stack,
        position: u64,
        source: Source,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        div()
            .id("orchestrator-stack-popover")
            .debug_selector(|| "orchestrator-stack-popover".into())
            .role(Role::Dialog)
            .aria_label(fl!("pr-stack-popover"))
            .occlude()
            .mt_1()
            .w(px(WIDTH))
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .shadow_lg()
            .overflow_hidden()
            .child(self.stack_verdict(pr, stack, position, source, cx))
            .child(self.stack_layers(stack, position, source, cx))
            .children(
                (source == Source::Github)
                    .then(|| self.stack_actions(pr, stack, position, cx))
                    .flatten(),
            )
    }

    /// The panel leading the popover: the merge queue entry while there is
    /// one, else what merging here would do.
    fn stack_verdict(
        &self,
        pr: &PullRequest,
        stack: &Stack,
        position: u64,
        source: Source,
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let verdict = stack.verdict(position);
        let (title, detail, open) = match (pr.queue, verdict) {
            (Some(queue), _) => {
                let title = fl!("pr-stack-queued", position = queue.position);
                let detail = match queue.eta {
                    Some(seconds) => format!(
                        "{} {}",
                        queue_detail(queue.state),
                        fl!("pr-stack-queue-eta", minutes = seconds.div_ceil(60).max(1))
                    ),
                    None => queue_detail(queue.state),
                };
                (title, detail, None)
            }
            (None, Verdict::Merged) => {
                (fl!("pr-stack-merged"), fl!("pr-stack-merged-detail"), None)
            }
            (None, Verdict::Ready { count }) => (
                fl!("pr-stack-ready", count = count),
                match source {
                    Source::Github => fl!("pr-stack-ready-detail", count = count),
                    Source::Inferred => fl!("pr-stack-inferred-ready-detail", count = count),
                },
                None,
            ),
            (None, Verdict::Unread) => {
                (fl!("pr-stack-unread"), fl!("pr-stack-unread-detail"), None)
            }
            (None, Verdict::Blocked { number, blocker }) => {
                let who = if number == pr.number {
                    fl!("pr-stack-this")
                } else {
                    format!("#{number}")
                };
                let elsewhere = (number != pr.number).then_some(number);
                (fl!("pr-stack-cant-merge"), why(blocker, who), elsewhere)
            }
        };
        let color = summary_color(pr, stack, position, theme);
        div()
            .debug_selector(|| "orchestrator-stack-verdict".into())
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .bg(rgb(mix(theme.surface, color, 12)))
            .child(
                div()
                    .text_size(px(look.ui.size + 2.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(div().text_color(rgb(theme.subtext())).child(detail))
            .children(open.map(|number| {
                div().pt_1().flex().child(
                    look.button(
                        "orchestrator-stack-open-blocker",
                        fl!("pr-stack-open", number = number),
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_stack_layer(number, cx);
                    })),
                )
            }))
    }

    /// Each layer as a step, bottom first, under a line naming the base.
    fn stack_layers(
        &self,
        stack: &Stack,
        position: u64,
        source: Source,
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let steps = stack.layers.iter().enumerate().map(|(index, layer)| {
            let current = layer.position == position;
            let number = layer.number;
            let color = tone_color(layer.tone(), theme);
            let hover = theme.active;
            div()
                .id(("orchestrator-stack-layer", index))
                .debug_selector(move || format!("orchestrator-stack-layer-{index}"))
                .role(Role::Link)
                .aria_label(format!("#{number} {}", layer.title))
                .flex_1()
                .min_w(px(STEP))
                .p_1()
                .flex()
                .flex_col()
                .gap_1()
                .rounded(px(corners::SMALL))
                .when(!current, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_stack_layer(number, cx);
                        }))
                })
                .child(div().h(px(4.)).rounded_full().bg(rgb(color)))
                .child(
                    div()
                        .text_size(look.small())
                        .truncate()
                        .text_color(rgb(if current {
                            theme.foreground
                        } else {
                            theme.subtext()
                        }))
                        .when(current, |el| el.font_weight(FontWeight::SEMIBOLD))
                        .child(format!("#{number}")),
                )
                .child(
                    div()
                        .text_size(look.small())
                        .truncate()
                        .text_color(rgb(color))
                        .child(status(layer)),
                )
        });
        div()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(look.muted(fl!(
                "pr-stack-summary",
                size = stack.size,
                base = stack.base.as_str()
            )))
            .when(source == Source::Inferred, |el| {
                el.child(
                    look.muted(fl!("pr-stack-inferred"))
                        .debug_selector(|| "orchestrator-stack-inferred".into()),
                )
            })
            .child(div().flex().flex_wrap().gap_1().children(steps))
            .when(stack.truncated(), |el| {
                el.child(look.muted(fl!(
                    "pr-stack-truncated",
                    count = stack.layers.len(),
                    size = stack.size
                )))
            })
    }

    /// Opens layer `number`'s page when the list has it, else its page on
    /// GitHub, beside the open one's.
    fn open_stack_layer(&mut self, number: u64, cx: &mut Context<Self>) {
        self.stack = Popover::default();
        let Some(item) = self
            .detail
            .as_ref()
            .and_then(|detail| self.item(&detail.key))
        else {
            cx.notify();
            return;
        };
        let mut key = item.key.clone();
        key.native_id = format!("pr/{number}");
        let key = key.canonical();
        if self.item(&key).is_some() {
            self.selected = Some(key.clone());
            self.open_detail(key, cx);
            return;
        }
        // The open pull request's URL was checked against its repository.
        if let Some((repository, _)) = self
            .open_pull_request()
            .and_then(|pr| pr.url.rsplit_once('/'))
        {
            cx.emit(Event::OpenUrl(format!("{repository}/{number}")));
        }
        cx.notify();
    }
}

/// The color of what merging here comes to: the queue entry's, else the
/// verdict's, which for a blocked stack is the blocking layer's own.
fn summary_color(
    pr: &PullRequest,
    stack: &Stack,
    position: u64,
    theme: &crate::config::Theme,
) -> u32 {
    if let Some(queue) = &pr.queue {
        return queue_color(queue, theme);
    }
    let tone = match stack.verdict(position) {
        Verdict::Merged => Tone::Merged,
        Verdict::Ready { .. } => Tone::Ready,
        Verdict::Unread => Tone::Muted,
        Verdict::Blocked { number, .. } => stack
            .layers
            .iter()
            .find(|layer| layer.number == number)
            .map_or(Tone::Attention, Layer::tone),
    };
    tone_color(tone, theme)
}

fn queue_color(queue: &QueueEntry, theme: &crate::config::Theme) -> u32 {
    tone_color(
        match queue.state {
            QueueState::Unmergeable => Tone::Attention,
            QueueState::Mergeable | QueueState::Locked => Tone::Ready,
            _ => Tone::Waiting,
        },
        theme,
    )
}

fn queue_detail(state: QueueState) -> String {
    match state {
        QueueState::AwaitingChecks => fl!("pr-stack-queue-awaiting-checks"),
        QueueState::Locked => fl!("pr-stack-queue-locked"),
        QueueState::Mergeable => fl!("pr-stack-queue-mergeable"),
        QueueState::Queued => fl!("pr-stack-queue-queued"),
        QueueState::Unmergeable => fl!("pr-stack-queue-unmergeable"),
        QueueState::Unknown => fl!("pr-stack-queue-unknown"),
    }
}

/// Why the layer `who` names cannot merge, as a sentence.
fn why(blocker: Blocker, who: String) -> String {
    let who = who.as_str();
    match blocker {
        Blocker::Closed => fl!("pr-stack-why-closed", who = who),
        Blocker::Draft => fl!("pr-stack-why-draft", who = who),
        Blocker::Conflicts => fl!("pr-stack-why-conflicts", who = who),
        Blocker::ChecksFailed => fl!("pr-stack-why-checks-failed", who = who),
        Blocker::ChangesRequested => fl!("pr-stack-why-changes-requested", who = who),
        Blocker::ChecksRunning => fl!("pr-stack-why-checks-running", who = who),
        Blocker::ReviewRequired => fl!("pr-stack-why-review-required", who = who),
        Blocker::Behind => fl!("pr-stack-why-behind", who = who),
        Blocker::Held => fl!("pr-stack-why-held", who = who),
        Blocker::Undecided => fl!("pr-stack-why-undecided", who = who),
    }
}

/// A layer's status in a word or two.
fn status(layer: &Layer) -> String {
    match layer.blocker() {
        None if layer.state == State::Merged => fl!("pr-stack-status-merged"),
        None => fl!("pr-stack-status-ready"),
        Some(Blocker::Closed) => fl!("pr-stack-status-closed"),
        Some(Blocker::Draft) => fl!("pr-stack-status-draft"),
        Some(Blocker::Conflicts) => fl!("pr-stack-status-conflicts"),
        Some(Blocker::ChecksFailed) => fl!("pr-stack-status-checks-failed"),
        Some(Blocker::ChangesRequested) => fl!("pr-stack-status-changes-requested"),
        Some(Blocker::ChecksRunning) => fl!("pr-stack-status-checks-running"),
        Some(Blocker::ReviewRequired) => fl!("pr-stack-status-review-required"),
        Some(Blocker::Behind) => fl!("pr-stack-status-behind"),
        Some(Blocker::Held) => fl!("pr-stack-status-held"),
        Some(Blocker::Undecided) => fl!("pr-stack-status-undecided"),
    }
}
