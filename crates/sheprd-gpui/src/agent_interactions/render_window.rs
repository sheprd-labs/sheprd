//! In-window persistent modals, runbooks, mini-diffs, secret-safe vault cards,
//! spotlights, drop-zones, and inline chat interaction status chips.

use super::{ActiveInteraction, InteractionStore, ParsedInteraction, Payload, vault};
use crate::SheprdWindow;
use gpui::{prelude::*, *};
use sheprd_chat_protocol::EntryId;
use sheprd_control_protocol::protocol::modal::ModalScope;
use std::time::{Duration, Instant};

impl SheprdWindow {
    /// Renders the active persistent modal / runbook / mini-diff / spotlight
    /// overlay for this window (called from `append_overlays`).
    pub(crate) fn render_agent_interactions(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu.page.is_some() || self.zen_active() {
            return None;
        }
        let store = InteractionStore::try_global(cx)?;
        let now = Instant::now();
        let (active, total, deck_pos) = {
            let s = store.read(cx);
            let items = s.for_window(window.window_handle().window_id(), now);
            if items.is_empty() {
                return None;
            }
            let idx = s.deck_index.min(items.len() - 1);
            (items[idx].clone(), items.len(), idx)
        };
        if matches!(active.parsed.payload, Payload::Countdown { .. }) {
            window.request_animation_frame();
        }
        Some(self.render_window_modal_card(store, active, deck_pos, total, now, cx))
    }

    /// Compact inline chip rendered inside an agent's chat message for an
    /// interaction directive (shows Open / Active / Dismissed state + actions).
    pub(crate) fn render_chat_interaction_chip(
        &self,
        pane_id: &str,
        entry: EntryId,
        parsed: &ParsedInteraction,
        cx: &mut Context<Self>,
    ) -> Div {
        let is_progress = matches!(parsed.payload, Payload::Progress { .. });
        let state = InteractionStore::try_global(cx).map_or(super::EntryModalState::None, |s| {
            s.read(cx).entry_modal_state(pane_id, entry, is_progress)
        });
        let accent = match &parsed.payload {
            Payload::Secret { .. } => rgb(0x8b6cf6),
            Payload::Progress { done: true, .. } => rgb(0x3ea866),
            _ if parsed.scope == ModalScope::All => rgb(0xe59a24),
            _ => rgb(self.theme.primary()),
        };
        let is_highlighted = !matches!(state, super::EntryModalState::None);
        let label = match &state {
            super::EntryModalState::Open(_) => format!(
                "{} active · {}",
                if parsed.scope == ModalScope::All {
                    "Global modal"
                } else {
                    "Window modal"
                },
                parsed.title
            ),
            super::EntryModalState::InlineOnly(_) => {
                format!("Action ready · {}", parsed.title)
            }
            super::EntryModalState::None => {
                format!("{} · {}", parsed.directive.kind_name(), parsed.title)
            }
        };
        let pane_owned = pane_id.to_owned();
        let parsed_reopen = parsed.clone();

        let actions = match state {
            super::EntryModalState::InlineOnly(id) => {
                let open_id = id.clone();
                let dismiss_id = id;
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .id(("chat-interaction-open", entry.0))
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(6.))
                            .bg(rgb(self.theme.primary()))
                            .text_color(rgb(0xffffff))
                            .text_size(px(self.config.ui.size * 0.85))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .child("Open")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let store = InteractionStore::global(cx);
                                let global_ok = this.config.chat.global_modals;
                                store.update(cx, |s, cx| {
                                    if s.open_card(&open_id, global_ok) {
                                        super::sync_hud_deferred(cx);
                                        cx.notify();
                                    }
                                });
                            })),
                    )
                    .child(
                        div()
                            .id(("chat-interaction-dismiss", entry.0))
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(6.))
                            .bg(rgb(self.theme.active))
                            .text_size(px(self.config.ui.size * 0.85))
                            .cursor_pointer()
                            .child("Dismiss")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                let store = InteractionStore::global(cx);
                                store.update(cx, |s, cx| {
                                    s.dismiss(&dismiss_id);
                                    cx.notify();
                                });
                            })),
                    )
            }
            super::EntryModalState::Open(id) => div().child(
                div()
                    .id(("chat-interaction-toggle", entry.0))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .bg(rgb(self.theme.active))
                    .text_size(px(self.config.ui.size * 0.85))
                    .cursor_pointer()
                    .child("Dismiss")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        let store = InteractionStore::global(cx);
                        store.update(cx, |s, cx| {
                            s.dismiss(&id);
                            cx.notify();
                        });
                    })),
            ),
            super::EntryModalState::None => div().child(
                div()
                    .id(("chat-interaction-toggle", entry.0))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .bg(rgb(self.theme.active))
                    .text_size(px(self.config.ui.size * 0.85))
                    .cursor_pointer()
                    .child("Show")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let store = InteractionStore::global(cx);
                        let now = Instant::now();
                        let volume = this.config.sound.volume;
                        let win_id = window.window_handle().window_id();
                        let (workspace_label, workspace_root) =
                            this.workspace_info_for_pane(&pane_owned);
                        let display_level = if parsed_reopen.scope == ModalScope::All
                            && this.config.chat.global_modals
                        {
                            super::EscalationLevel::GlobalHud
                        } else {
                            super::EscalationLevel::WindowModal
                        };
                        store.update(cx, |s, cx| {
                            s.upsert(super::UpsertInteraction {
                                source: super::SourceKey {
                                    window_id: Some(win_id),
                                    pane_id: Some(pane_owned.clone()),
                                },
                                agent_label: "Agent".to_owned(),
                                workspace_label,
                                workspace_root,
                                entry_id: Some(entry),
                                source_revision: None,
                                directive_index: None,
                                parsed: parsed_reopen.clone(),
                                display_level,
                                manual_open: true,
                                now,
                                volume,
                            });
                            super::sync_hud_deferred(cx);
                            cx.notify();
                        });
                    })),
            ),
        };

        div()
            .rounded(px(8.))
            .px(px(10.))
            .py(px(7.))
            .bg(rgb(self.theme.surface))
            .border_1()
            .border_color(if is_highlighted {
                accent
            } else {
                rgb(self.theme.active)
            })
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(self.config.ui.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(self.theme.foreground))
                            .child(label),
                    )
                    .when(!parsed.body.is_empty(), |col| {
                        col.child(
                            div()
                                .text_size(px(self.config.ui.size * 0.85))
                                .text_color(rgb(self.theme.muted))
                                .line_clamp(2)
                                .child(parsed.body.clone()),
                        )
                    }),
            )
            .child(actions)
    }

    fn render_window_modal_card(
        &self,
        store: Entity<InteractionStore>,
        item: ActiveInteraction,
        deck_pos: usize,
        total: usize,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let urgent = item.parsed.scope == ModalScope::All;
        let accent = match &item.parsed.payload {
            Payload::Secret { .. } => rgb(0x8b6cf6),
            Payload::Spotlight { .. } => rgb(theme.primary()),
            _ if urgent => rgb(0xe59a24),
            _ => rgb(theme.primary()),
        };
        let elapsed = now.saturating_duration_since(item.created_at).as_secs();
        let timer = format!("{}:{:02}", elapsed / 60, elapsed % 60);

        let id_dismiss = item.id.clone();
        let id_snooze = item.id.clone();
        let pane_jump = item.source.pane_id.clone();
        let store_dismiss = store.clone();
        let store_snooze = store.clone();
        let store_prev = store.clone();
        let store_next = store.clone();

        let body_section = self.render_payload_body(store.clone(), &item, now, cx);

        div()
            .absolute()
            .top(px(self.tab_strip_height() + 14.))
            .left(px(20.))
            .right(px(20.))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("agent-window-modal")
                    .max_w(px(520.))
                    .w_full()
                    .rounded(px(12.))
                    .bg(rgb(theme.surface))
                    .border_2()
                    .border_color(accent)
                    .shadow_lg()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    // Header row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(11.5))
                            .text_color(rgb(theme.muted))
                            .child(
                                div()
                                    .px(px(6.))
                                    .py(px(2.))
                                    .rounded(px(4.))
                                    .bg(rgb(theme.active))
                                    .text_color(rgb(theme.foreground))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if urgent { "Urgent" } else { "Pinned" }),
                            )
                            .child(format!("{} · {}", item.agent_label, item.workspace_label))
                            .when(total > 1, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .child(
                                            div()
                                                .id("modal-deck-prev")
                                                .px(px(5.))
                                                .cursor_pointer()
                                                .child("‹")
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    store_prev.update(cx, |s, cx| {
                                                        s.step_deck(-1, total);
                                                        cx.notify();
                                                    });
                                                })),
                                        )
                                        .child(format!("{} of {total}", deck_pos + 1))
                                        .child(
                                            div()
                                                .id("modal-deck-next")
                                                .px(px(5.))
                                                .cursor_pointer()
                                                .child("›")
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    store_next.update(cx, |s, cx| {
                                                        s.step_deck(1, total);
                                                        cx.notify();
                                                    });
                                                })),
                                        ),
                                )
                            })
                            .child(div().ml_auto().child(timer)),
                    )
                    // Title
                    .child(
                        div()
                            .text_size(px(self.config.ui.size * 1.15))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.foreground))
                            .child(item.parsed.title.clone()),
                    )
                    // Optional prose body
                    .when(!item.parsed.body.is_empty(), |col| {
                        col.child(
                            div()
                                .text_size(px(self.config.ui.size))
                                .text_color(rgb(theme.subtext()))
                                .child(item.parsed.body.clone()),
                        )
                    })
                    // Payload-specific interactive section
                    .when_some(body_section, |col, section| col.child(section))
                    // Optional transient feedback line
                    .when_some(item.feedback.clone(), |col, feedback| {
                        col.child(
                            div()
                                .px(px(9.))
                                .py(px(6.))
                                .rounded(px(6.))
                                .bg(rgb(theme.active))
                                .text_size(px(12.))
                                .text_color(rgb(0x67d9a6))
                                .child(feedback),
                        )
                    })
                    // Footer actions
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                div()
                                    .id("modal-snooze")
                                    .px(px(10.))
                                    .py(px(5.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(rgb(theme.active))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme.active)))
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
                                    .id("modal-dismiss")
                                    .px(px(10.))
                                    .py(px(5.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(rgb(theme.active))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme.active)))
                                    .child("Dismiss")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        store_dismiss.update(cx, |s, cx| {
                                            s.dismiss(&id_dismiss);
                                            cx.notify();
                                        });
                                    })),
                            )
                            .when_some(pane_jump, |row, pane| {
                                row.child(
                                    div()
                                        .id("modal-jump")
                                        .px(px(12.))
                                        .py(px(5.))
                                        .rounded(px(6.))
                                        .bg(rgb(theme.primary()))
                                        .text_color(rgb(0xffffff))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .child("Go to chat")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.focus_chat_pane_by_id(&pane, cx);
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_payload_body(
        &self,
        store: Entity<InteractionStore>,
        item: &ActiveInteraction,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = &self.theme;
        match &item.parsed.payload {
            Payload::Modal { choices, .. } if !choices.is_empty() => {
                let pane = item.source.pane_id.clone();
                let item_id = item.id.clone();
                Some(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(choices.iter().enumerate().map(|(i, choice)| {
                            let choice_text = choice.clone();
                            let pane = pane.clone();
                            let item_id = item_id.clone();
                            let store = store.clone();
                            div()
                                .id(("modal-choice", i))
                                .px(px(10.))
                                .py(px(5.))
                                .rounded(px(6.))
                                .bg(rgb(theme.active))
                                .cursor_pointer()
                                .text_size(px(12.))
                                .child(choice.clone())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(pane) = pane.as_deref() {
                                        this.send_interaction_reply(pane, &choice_text, cx);
                                    }
                                    store.update(cx, |s, cx| {
                                        s.dismiss(&item_id);
                                        cx.notify();
                                    });
                                }))
                        }))
                        .into_any_element(),
                )
            }
            Payload::Countdown { seconds } => {
                let elapsed = now.saturating_duration_since(item.created_at).as_secs();
                let left = u64::from(*seconds).saturating_sub(elapsed);
                let pct = if *seconds == 0 {
                    0.0
                } else {
                    (left as f32 / *seconds as f32).clamp(0.0, 1.0)
                };
                let pane = item.source.pane_id.clone();
                let item_id = item.id.clone();
                let store_cancel = store.clone();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div()
                                .w_full()
                                .h(px(6.))
                                .rounded(px(99.))
                                .bg(rgb(theme.active))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(480. * pct))
                                        .bg(rgb(0xe59a24)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(0xf8ca70))
                                        .child(format!("Proceeding in {left}s")),
                                )
                                .child(
                                    div()
                                        .id("countdown-cancel")
                                        .px(px(9.))
                                        .py(px(4.))
                                        .rounded(px(6.))
                                        .bg(rgba(0xe05a5a33))
                                        .text_color(rgb(0xffb4b4))
                                        .cursor_pointer()
                                        .child("Cancel action")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(pane) = pane.as_deref() {
                                                this.send_interaction_reply(
                                                    pane,
                                                    "Cancel the scheduled action.",
                                                    cx,
                                                );
                                            }
                                            store_cancel.update(cx, |s, cx| {
                                                s.dismiss(&item_id);
                                                cx.notify();
                                            });
                                        })),
                                ),
                        )
                        .into_any_element(),
                )
            }
            Payload::Secret { key, dest, .. } => {
                let key_owned = key.clone();
                let dest_owned = dest.clone();
                let item_id = item.id.clone();
                let pane = item.source.pane_id.clone();
                let item_workspace_root = item.workspace_root.clone();
                let store_paste = store.clone();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div()
                                .px(px(9.))
                                .py(px(6.))
                                .rounded(px(6.))
                                .bg(rgb(theme.active))
                                .text_size(px(11.5))
                                .child(format!(
                                    "🔒 {key} → {dest} (never enters chat history or LLM context)"
                                )),
                        )
                        .child(
                            div()
                                .id("secret-paste-inject")
                                .self_start()
                                .px(px(11.))
                                .py(px(6.))
                                .rounded(px(7.))
                                .bg(rgb(0x8b6cf6))
                                .text_color(rgb(0xffffff))
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .child("Inject from clipboard securely")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let Some(raw) = cx.read_from_clipboard().and_then(|c| c.text())
                                    else {
                                        store_paste.update(cx, |s, cx| {
                                            s.set_feedback(
                                                &item_id,
                                                "Clipboard is empty — copy the secret first".into(),
                                            );
                                            cx.notify();
                                        });
                                        return;
                                    };
                                    let root = item_workspace_root.clone().or_else(|| {
                                        pane.as_deref()
                                            .and_then(|p| this.workspace_path_for_pane(p))
                                    });
                                    let key = key_owned.clone();
                                    let dest = dest_owned.clone();
                                    let item_id = item_id.clone();
                                    let pane = pane.clone();
                                    let store = store_paste.clone();
                                    cx.spawn(async move |this, cx| {
                                        let result = cx
                                            .background_executor()
                                            .spawn(async move {
                                                let secret = vault::SecretValue::new(raw)?;
                                                let parsed_dest = vault::parse_destination(&dest)?;
                                                vault::inject(
                                                    root.as_deref(),
                                                    &key,
                                                    &parsed_dest,
                                                    &secret,
                                                )
                                            })
                                            .await;
                                        cx.update(|cx| match result {
                                            Ok(receipt) => {
                                                if let Some(pane) = pane.as_deref() {
                                                    let _ = this.update(cx, |win, cx| {
                                                        win.send_interaction_reply(
                                                            pane,
                                                            &receipt.summary,
                                                            cx,
                                                        );
                                                    });
                                                }
                                                store.update(cx, |s, cx| {
                                                    s.set_feedback(&item_id, receipt.summary);
                                                    cx.notify();
                                                });
                                            }
                                            Err(err) => {
                                                store.update(cx, |s, cx| {
                                                    s.set_feedback(&item_id, err.to_string());
                                                    cx.notify();
                                                });
                                            }
                                        });
                                    })
                                    .detach();
                                })),
                        )
                        .into_any_element(),
                )
            }
            Payload::Runbook { steps } => {
                let item_id = item.id.clone();
                let pane = item.source.pane_id.clone();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .children(steps.iter().enumerate().map(|(idx, step)| {
                            let item_id = item_id.clone();
                            let pane = pane.clone();
                            let store = store.clone();
                            let mark = if step.done { "✓" } else { "○" };
                            let owner = if step.human { "You" } else { "Agent" };
                            div()
                                .id(("runbook-step", idx))
                                .px(px(8.))
                                .py(px(6.))
                                .rounded(px(6.))
                                .bg(rgb(theme.active))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .when(step.human, |row| row.cursor_pointer())
                                .child(mark)
                                .child(div().flex_1().child(step.label.clone()))
                                .child(div().text_size(px(11.)).text_color(rgb(theme.muted)).child(owner))
                                .when(step.human, |row| {
                                    row.on_click(cx.listener(move |this, _, _, cx| {
                                        let toggled = store.update(cx, |s, cx| {
                                            let res = s.toggle_runbook_step(&item_id, idx);
                                            cx.notify();
                                            res
                                        });
                                        if let (Some(step), Some(pane)) = (toggled, pane.as_deref())
                                            && step.done
                                        {
                                            this.send_interaction_reply(
                                                pane,
                                                &format!("Completed runbook step: {}", step.label),
                                                cx,
                                            );
                                        }
                                    }))
                                })
                        }))
                        .into_any_element(),
                )
            }
            Payload::Diff { file, diff } => {
                let item_id = item.id.clone();
                let pane = item.source.pane_id.clone();
                let store_a = store.clone();
                let store_r = store.clone();
                let id_r = item_id.clone();
                let pane_r = pane.clone();
                let lines: Vec<&str> = diff.lines().collect();
                let added = lines.iter().filter(|l| l.starts_with('+')).count();
                let removed = lines.iter().filter(|l| l.starts_with('-')).count();
                let meta = if file.is_empty() {
                    format!("{} lines (+{added} / -{removed})", lines.len())
                } else {
                    format!("{file} · {} lines (+{added} / -{removed})", lines.len())
                };
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(theme.muted))
                                .child(meta),
                        )
                        .child(
                            div()
                                .id("diff-lines-scroll")
                                .max_h(px(220.))
                                .overflow_y_scroll()
                                .p(px(8.))
                                .rounded(px(6.))
                                .bg(rgb(theme.background))
                                .font_family(self.config.terminal.font().family.clone())
                                .text_size(px(11.5))
                                .flex()
                                .flex_col()
                                .children(lines.into_iter().map(|line| {
                                    let color = if line.starts_with('+') {
                                        rgb(0x8ef0b0)
                                    } else if line.starts_with('-') {
                                        rgb(0xff9c9c)
                                    } else {
                                        rgb(theme.muted)
                                    };
                                    div().text_color(color).child(line.to_owned())
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .id("diff-reject")
                                        .px(px(10.))
                                        .py(px(5.))
                                        .rounded(px(6.))
                                        .bg(rgba(0xe05a5a33))
                                        .text_color(rgb(0xffb4b4))
                                        .cursor_pointer()
                                        .child("Reject")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(pane) = pane_r.as_deref() {
                                                this.send_interaction_reply(
                                                    pane,
                                                    "Rejected proposed diff.",
                                                    cx,
                                                );
                                            }
                                            store_r.update(cx, |s, cx| {
                                                s.dismiss(&id_r);
                                                cx.notify();
                                            });
                                        })),
                                )
                                .child(
                                    div()
                                        .id("diff-approve")
                                        .px(px(10.))
                                        .py(px(5.))
                                        .rounded(px(6.))
                                        .bg(rgb(0x3ea866))
                                        .text_color(rgb(0xffffff))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .child("Approve & apply")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(pane) = pane.as_deref() {
                                                this.send_interaction_reply(
                                                    pane,
                                                    "Approved proposed diff — proceed.",
                                                    cx,
                                                );
                                            }
                                            store_a.update(cx, |s, cx| {
                                                s.dismiss(&item_id);
                                                cx.notify();
                                            });
                                        })),
                                ),
                        )
                        .into_any_element(),
                )
            }
            Payload::Oauth { url, code } => {
                let open_url = url.clone();
                let copy_code = code.clone();
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .when_some(copy_code, |row, code| {
                            row.child(
                                div()
                                    .px(px(8.))
                                    .py(px(4.))
                                    .rounded(px(6.))
                                    .bg(rgb(theme.active))
                                    .font_weight(FontWeight::BOLD)
                                    .child(code),
                            )
                        })
                        .when(!open_url.is_empty(), |row| {
                            row.child(
                                div()
                                    .id("oauth-open-url")
                                    .px(px(10.))
                                    .py(px(5.))
                                    .rounded(px(6.))
                                    .bg(rgb(theme.primary()))
                                    .text_color(rgb(0xffffff))
                                    .cursor_pointer()
                                    .child("Open sign-in page ↗")
                                    .on_click(move |_, _, cx| {
                                        if open_url.starts_with("https://")
                                            || open_url.starts_with("http://")
                                        {
                                            cx.open_url(&open_url);
                                        }
                                    }),
                            )
                        })
                        .into_any_element(),
                )
            }
            Payload::Spotlight { target, step } => Some(
                div()
                    .px(px(9.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(theme.active))
                    .text_size(px(11.5))
                    .child(format!(
                        "Spotlight target: {target}{}",
                        step.as_deref()
                            .map_or(String::new(), |s| format!(" · step {s}"))
                    ))
                    .into_any_element(),
            ),
            Payload::Drop { accept } => Some(
                div()
                    .p(px(12.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(theme.primary()))
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(if accept.is_empty() {
                        "Drag and drop a file onto this chat window to attach it.".to_owned()
                    } else {
                        format!("Drag and drop a file ({accept}) onto this chat window to attach.")
                    })
                    .into_any_element(),
            ),
            Payload::Otp => Some(
                div()
                    .px(px(9.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(theme.active))
                    .text_size(px(11.5))
                    .child("2FA code required — open the waiting chat or terminal to enter your one-time code.")
                    .into_any_element(),
            ),
            _ => None,
        }
    }
}
