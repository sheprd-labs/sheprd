//! Global always-on-top floating HUD and Dynamic Progress Pill window.
//! On macOS, configures the underlying `NSWindow` at `NSFloatingWindowLevel`
//! with `CanJoinAllSpaces | FullScreenAuxiliary` so urgent blockers (such as
//! YubiKey touch) and progress pills remain visible across all apps and Spaces
//! without stealing keyboard focus.

use super::{ActiveInteraction, InteractionStore, Payload};
use crate::window::main_window::MainWindow;
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

const HUD_WIDTH: f32 = 420.;
const HUD_HEIGHT: f32 = 168.;
const TOP_INSET: f32 = 44.;

/// Floating GPUI window view observing [`InteractionStore`].
pub(crate) struct GlobalHud {
    store: Entity<InteractionStore>,
    _observe: Subscription,
}

/// Opens the global always-on-top HUD window if not already open.
pub(crate) fn open(store: Entity<InteractionStore>, cx: &mut App) {
    if store.read(cx).hud_open {
        return;
    }
    store.update(cx, |s, _| s.hud_open = true);
    let bounds = hud_bounds(cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        configure_platform_window(window);
        cx.new(|cx| GlobalHud::new(store.clone(), window, cx))
    });
    if let Err(error) = opened {
        tracing::warn!(%error, "Cannot open global agent HUD window");
        store.update(cx, |s, _| s.hud_open = false);
    }
}

fn hud_bounds(cx: &App) -> Bounds<Pixels> {
    if let Some(display) = cx.primary_display() {
        let db = display.bounds();
        let x = db.origin.x + (db.size.width - px(HUD_WIDTH)) * 0.5;
        let y = db.origin.y + px(TOP_INSET);
        return Bounds::new(point(x, y), size(px(HUD_WIDTH), px(HUD_HEIGHT)));
    }
    Bounds::new(
        point(px(320.), px(TOP_INSET)),
        size(px(HUD_WIDTH), px(HUD_HEIGHT)),
    )
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn configure_platform_window(window: &Window) {
    use objc2_app_kit::{NSFloatingWindowLevel, NSView, NSWindowCollectionBehavior};
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    // SAFETY: GPUI's handle names the window's content view, alive for this
    // main-thread constructor call; no safe GPUI accessor exposes NSWindow.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let Some(ns_win) = view.window() else {
        return;
    };
    ns_win.setLevel(NSFloatingWindowLevel);
    ns_win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    ns_win.setHasShadow(false);
    ns_win.orderFrontRegardless();
}

#[cfg(not(target_os = "macos"))]
fn configure_platform_window(_window: &Window) {}

impl GlobalHud {
    fn new(store: Entity<InteractionStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe_in(&store, window, |_, store, window, cx| {
            let now = Instant::now();
            let empty = store.read(cx).global_hud_items(now).is_empty();
            if empty {
                store.update(cx, |s, _| s.hud_open = false);
                window.remove_window();
            } else {
                cx.notify();
            }
        });
        Self {
            store,
            _observe: observe,
        }
    }

    fn focus_source(&self, item: &ActiveInteraction, cx: &mut Context<Self>) {
        let target_window = item.source.window_id;
        let pane_id = item.source.pane_id.clone();
        cx.activate(true);
        for main in MainWindow::all(cx) {
            if target_window.is_none_or(|id| main.window_id() == id) {
                let _ = main.update(cx, |win, window, cx| {
                    window.activate_window();
                    if let Some(pane) = pane_id.as_deref() {
                        win.focus_chat_pane_by_id(pane, cx);
                    }
                    cx.notify();
                });
                break;
            }
        }
    }
}

impl Render for GlobalHud {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.store.update(cx, |s, _| s.prune_expired(now));
        let (blocker, progress, expanded, total_blockers) = {
            let s = self.store.read(cx);
            let blockers: Vec<&ActiveInteraction> = s
                .global_hud_items(now)
                .into_iter()
                .filter(|item| !item.is_progress())
                .collect();
            let blocker = blockers.last().copied().cloned();
            let progress: Vec<ActiveInteraction> =
                s.progress_items(now).into_iter().cloned().collect();
            (blocker, progress, s.pill_expanded, blockers.len())
        };
        if blocker.is_none() && progress.is_empty() {
            self.store.update(cx, |s, _| s.hud_open = false);
            window.remove_window();
            return div().into_any_element();
        }
        // Keep timers/countdowns fresh while a HUD is visible.
        window.request_animation_frame();

        if let Some(item) = blocker {
            return self.render_blocker_hud(item, total_blockers, now, cx);
        }
        self.render_progress_pill(progress, expanded, cx)
    }
}

impl GlobalHud {
    fn render_blocker_hud(
        &self,
        item: ActiveInteraction,
        total: usize,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let elapsed = now.saturating_duration_since(item.created_at).as_secs();
        let mins = elapsed / 60;
        let secs = elapsed % 60;
        let id_dismiss = item.id.clone();
        let id_snooze = item.id.clone();
        let jump_item = item.clone();
        let store = self.store.clone();
        let store_snooze = self.store.clone();

        let accent = match &item.parsed.payload {
            Payload::Secret { .. } => rgb(0x8b6cf6),
            _ => rgb(0xe59a24),
        };
        let badge = match &item.parsed.payload {
            Payload::Secret { .. } => "🔒",
            Payload::Otp => "🔐",
            Payload::Oauth { .. } => "↗",
            Payload::Drop { .. } => "⬇",
            Payload::Countdown { .. } => "⏱",
            _ => "🔑",
        };
        let count_tag = (total > 1).then(|| format!("1 of {total}"));

        div()
            .size_full()
            .p(px(10.))
            .flex()
            .justify_center()
            .items_start()
            .child(
                div()
                    .w_full()
                    .rounded(px(14.))
                    .bg(rgba(0x191c24f5))
                    .border_2()
                    .border_color(accent)
                    .shadow_lg()
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .text_color(rgb(0xf4f6fb))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(rgb(0xa8b0c2))
                            .child(
                                div()
                                    .px(px(6.))
                                    .py(px(2.))
                                    .rounded(px(4.))
                                    .bg(rgba(0xffffff1f))
                                    .text_color(rgb(0xffffff))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Sheprd"),
                            )
                            .child(format!("{} · {}", item.agent_label, item.workspace_label))
                            .when_some(count_tag, |row, tag| {
                                row.child(
                                    div()
                                        .px(px(5.))
                                        .py(px(1.))
                                        .rounded(px(4.))
                                        .bg(rgba(0xffffff14))
                                        .child(tag),
                                )
                            })
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(0xf8ca70))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("BLOCKED · {mins}:{secs:02}")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .w(px(36.))
                                    .h(px(36.))
                                    .rounded(px(10.))
                                    .bg(rgba(0xe59a2429))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(18.))
                                    .child(badge),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0xffffff))
                                            .child(item.parsed.title.clone()),
                                    )
                                    .when(!item.parsed.body.is_empty(), |col| {
                                        col.child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(rgb(0xbec5d4))
                                                .line_clamp(2)
                                                .child(item.parsed.body.clone()),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("hud-snooze")
                                    .px(px(9.))
                                    .py(px(5.))
                                    .rounded(px(7.))
                                    .bg(rgba(0xffffff14))
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgba(0xffffff24)))
                                    .child("Snooze 2m")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        store_snooze.update(cx, |s, cx| {
                                            s.snooze_and_schedule(
                                                &id_snooze,
                                                Duration::from_secs(120),
                                                Instant::now(),
                                                cx,
                                            );
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .id("hud-dismiss")
                                    .px(px(9.))
                                    .py(px(5.))
                                    .rounded(px(7.))
                                    .bg(rgba(0xffffff14))
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgba(0xffffff24)))
                                    .child("Dismiss")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        store.update(cx, |s, cx| {
                                            s.dismiss(&id_dismiss);
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .id("hud-open")
                                    .px(px(10.))
                                    .py(px(5.))
                                    .rounded(px(7.))
                                    .bg(rgb(0x4a78e6))
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x5a87f2)))
                                    .child("Open chat")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.focus_source(&jump_item, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_progress_pill(
        &self,
        items: Vec<ActiveInteraction>,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(first) = items.first().cloned() else {
            return div().into_any_element();
        };
        let (percent, step, done) = match &first.parsed.payload {
            Payload::Progress {
                percent,
                step,
                done,
            } => (*percent, step.clone(), *done),
            _ => (None, String::new(), false),
        };
        let store_toggle = self.store.clone();
        let store_dismiss = self.store.clone();
        let id_dismiss = first.id.clone();
        let jump_item = first.clone();

        if done {
            return div()
                .size_full()
                .p(px(10.))
                .flex()
                .justify_center()
                .items_start()
                .child(
                    div()
                        .px(px(16.))
                        .py(px(9.))
                        .rounded(px(99.))
                        .bg(rgb(0x173322))
                        .border_1()
                        .border_color(rgb(0x3ea866))
                        .text_color(rgb(0xc2f7d5))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .shadow_lg()
                        .child(format!("✓ {}", first.parsed.title)),
                )
                .into_any_element();
        }

        let pct = percent.unwrap_or(50).min(100);
        let pct_label = percent.map_or_else(|| "…".to_owned(), |p| format!("{p}%"));

        if !expanded && items.len() == 1 {
            return div()
                .size_full()
                .p(px(10.))
                .flex()
                .justify_center()
                .items_start()
                .child(
                    div()
                        .id("dynamic-pill-compact")
                        .px(px(14.))
                        .py(px(8.))
                        .rounded(px(99.))
                        .bg(rgba(0x10131af5))
                        .border_1()
                        .border_color(rgba(0xffffff26))
                        .text_color(rgb(0xf2f4fa))
                        .text_size(px(12.))
                        .shadow_lg()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            store_toggle.update(cx, |s, cx| {
                                s.pill_expanded = !s.pill_expanded;
                                cx.notify();
                            });
                        }))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{} · {}", first.agent_label, first.parsed.title)),
                        )
                        .child(
                            div()
                                .w(px(64.))
                                .h(px(5.))
                                .rounded(px(99.))
                                .bg(rgba(0xffffff26))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(64. * f32::from(pct) / 100.))
                                        .bg(rgb(0x64b5ff)),
                                ),
                        )
                        .child(div().text_color(rgb(0x9ea7ba)).child(pct_label)),
                )
                .into_any_element();
        }

        div()
            .size_full()
            .p(px(10.))
            .flex()
            .justify_center()
            .items_start()
            .child(
                div()
                    .w(px(356.))
                    .rounded(px(16.))
                    .bg(rgba(0x10131af5))
                    .border_1()
                    .border_color(rgba(0xffffff26))
                    .text_color(rgb(0xf2f4fa))
                    .p(px(12.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(12.5))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(
                                if items.len() > 1 {
                                    format!("{} agents running", items.len())
                                } else {
                                    format!("{} · {}", first.agent_label, first.parsed.title)
                                },
                            ))
                            .child(div().text_color(rgb(0x7fb7ff)).child(pct_label)),
                    )
                    .when(!step.is_empty(), |col| {
                        col.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x9ea7ba))
                                .child(step),
                        )
                    })
                    .child(
                        div()
                            .w_full()
                            .h(px(5.))
                            .rounded(px(99.))
                            .bg(rgba(0xffffff1f))
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(px(332. * f32::from(pct) / 100.))
                                    .bg(rgb(0x64b5ff)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("pill-collapse")
                                    .px(px(8.))
                                    .py(px(4.))
                                    .rounded(px(6.))
                                    .bg(rgba(0xffffff14))
                                    .text_size(px(11.))
                                    .cursor_pointer()
                                    .child("Collapse")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        store_toggle.update(cx, |s, cx| {
                                            s.pill_expanded = false;
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .id("pill-hide")
                                            .px(px(8.))
                                            .py(px(4.))
                                            .rounded(px(6.))
                                            .bg(rgba(0xffffff14))
                                            .text_size(px(11.))
                                            .cursor_pointer()
                                            .child("Hide")
                                            .on_click(cx.listener(move |_, _, _, cx| {
                                                store_dismiss.update(cx, |s, cx| {
                                                    s.dismiss(&id_dismiss);
                                                    cx.notify();
                                                });
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("pill-open")
                                            .px(px(9.))
                                            .py(px(4.))
                                            .rounded(px(6.))
                                            .bg(rgb(0x4a78e6))
                                            .text_color(rgb(0xffffff))
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .cursor_pointer()
                                            .child("Open chat ↗")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.focus_source(&jump_item, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
