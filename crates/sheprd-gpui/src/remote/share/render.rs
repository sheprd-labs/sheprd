//! Rendering for the "Share this session" modal dialog.
// i18n: migrated

use super::{BusyState, LIFETIMES, PreviewState, ShareDialog};
use crate::{
    SheprdWindow,
    config::{Theme, corners},
    i18n::fl,
    remote::dialog::{Tone, button},
};
use gpui::{prelude::*, *};
use sheprd_remote_control::control::sharing::{PreviewRole, ShareHistory, ShareSummary};

impl SheprdWindow {
    pub(in crate::remote) fn render_share_session(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(dialog) = &self.menu.remote_share else {
            return div();
        };
        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                svg()
                                    .path(
                                        crate::icons::AgentIcon::from_identity(Some(
                                            dialog.agent.to_ascii_lowercase().as_str(),
                                        ))
                                        .path(),
                                    )
                                    .size(px(14.))
                                    .flex_none()
                                    .text_color(rgb(theme.foreground)),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(fl!("remote-share-title")),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px((self.config.ui.size - 1.).max(10.)))
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("remote-share-subtitle")),
                    ),
            )
            .child(
                div()
                    .id("remote-share-close-x")
                    .size(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(corners::CONTROL))
                    .cursor_pointer()
                    .hover(|btn| btn.bg(rgb(theme.active)))
                    .child(
                        svg()
                            .path("icons/close.svg")
                            .size(px(12.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss_menu(window, cx);
                    })),
            );

        if !dialog.enabled {
            return div()
                .debug_selector(|| "remote-share-dialog".into())
                .flex()
                .flex_col()
                .p(px(12.))
                .gap(px(12.))
                .child(header)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .p(px(10.))
                        .rounded(px(corners::CONTROL))
                        .bg(rgb(theme.background))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(fl!("remote-share-off")),
                        )
                        .child(
                            div()
                                .text_color(rgb(theme.subtext()))
                                .child(fl!("remote-share-off-detail")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            button(
                                theme,
                                "remote-share-close",
                                fl!("remote-share-close"),
                                Tone::Plain,
                                true,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_menu(window, cx);
                                },
                            )),
                        )
                        .child(
                            button(
                                theme,
                                "remote-share-settings",
                                fl!("remote-share-open-settings"),
                                Tone::Primary,
                                true,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_menu(window, cx);
                                    crate::settings_window::open_remote(
                                        cx.entity().downgrade(),
                                        cx,
                                    );
                                },
                            )),
                        ),
                );
        }

        let small = px((self.config.ui.size - 1.).max(10.));
        let busy = dialog.busy != BusyState::Idle;
        let can_create = !busy && !dialog.relays.is_empty();

        div()
            .debug_selector(|| "remote-share-dialog".into())
            .flex()
            .flex_col()
            .p(px(12.))
            .gap(px(10.))
            .child(header)
            .child(self.render_share_access_card(theme, small))
            .child(self.render_share_history_row(dialog, theme, small, cx))
            .child(self.render_share_tools_row(dialog, theme, small, cx))
            .child(self.render_share_expiry_row(dialog, theme, small, cx))
            .child(self.render_share_relay_row(dialog, theme, small, cx))
            .child(self.render_share_preview(dialog, theme, small, cx))
            .children(
                dialog
                    .created
                    .as_ref()
                    .map(|created| self.render_share_created(dialog, created, theme, small, cx)),
            )
            .children(
                (!dialog.active.is_empty())
                    .then(|| self.render_share_active(dialog, theme, small, cx)),
            )
            .children(dialog.error.as_ref().map(|error| {
                div()
                    .debug_selector(|| "remote-share-error".into())
                    .text_size(small)
                    .text_color(crate::menu::danger(theme))
                    .child(crate::notifications::safe_text(error, 240))
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(2.))
                    .child(
                        button(
                            theme,
                            "remote-share-close",
                            fl!("remote-share-close"),
                            Tone::Plain,
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_menu(window, cx);
                        })),
                    )
                    .child(
                        button(
                            theme,
                            "remote-share-create",
                            if dialog.busy == BusyState::Creating {
                                fl!("remote-share-creating")
                            } else {
                                fl!("remote-share-create")
                            },
                            Tone::Primary,
                            can_create,
                        )
                        .when(can_create, |btn| {
                            btn.on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.create_share_link(cx);
                            }))
                        }),
                    ),
            )
    }

    fn render_share_access_card(&self, theme: &Theme, small: Pixels) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .p(px(8.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.background))
            .border_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("remote-share-permissions-label")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                svg()
                                    .path("icons/lock.svg")
                                    .size(px(11.))
                                    .text_color(rgb(theme.foreground)),
                            )
                            .child(fl!("remote-share-view-only")),
                    ),
            )
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-view-only-detail")),
            )
    }

    fn render_share_history_row(
        &self,
        dialog: &ShareDialog,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let from_now = dialog.history == ShareHistory::FromNow;
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-history-label")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(
                        segment(
                            theme,
                            "remote-share-history-from-now",
                            fl!("remote-share-history-from-now"),
                            from_now,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_share_history(ShareHistory::FromNow, cx);
                        })),
                    )
                    .child(
                        segment(
                            theme,
                            "remote-share-history-all",
                            fl!("remote-share-history-all"),
                            !from_now,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_share_history(ShareHistory::All, cx);
                        })),
                    ),
            )
    }

    fn render_share_tools_row(
        &self,
        dialog: &ShareDialog,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let include = dialog.include_tools;
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-tools-label")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(
                        segment(
                            theme,
                            "remote-share-tools-off",
                            fl!("remote-share-tools-off"),
                            !include,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_share_tools(false, cx);
                        })),
                    )
                    .child(
                        segment(
                            theme,
                            "remote-share-tools-on",
                            fl!("remote-share-tools-on"),
                            include,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_share_tools(true, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-tools-note")),
            )
    }

    fn render_share_expiry_row(
        &self,
        dialog: &ShareDialog,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-expiry-label")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .children(LIFETIMES.iter().map(|&(secs, _)| {
                        let label = match secs {
                            900 => fl!("remote-share-expiry-15m"),
                            1800 => fl!("remote-share-expiry-30m"),
                            _ => fl!("remote-share-expiry-1h"),
                        };
                        let id = match secs {
                            900 => "remote-share-expiry-15m",
                            1800 => "remote-share-expiry-30m",
                            _ => "remote-share-expiry-1h",
                        };
                        segment(theme, id, label, dialog.lifetime_secs == secs).on_click(
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.set_share_lifetime(secs, cx);
                            }),
                        )
                    })),
            )
    }

    fn render_share_relay_row(
        &self,
        dialog: &ShareDialog,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = div().flex().flex_col().gap(px(4.)).child(
            div()
                .text_size(small)
                .text_color(rgb(theme.subtext()))
                .child(fl!("remote-share-relay-label")),
        );
        if let Some(relay) = dialog.selected_relay() {
            let label = relay.strip_prefix("https://").unwrap_or(relay).to_owned();
            let clickable = dialog.relays.len() > 1;
            row = row.child(
                div()
                    .id("remote-share-relay")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.background))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .when(clickable, |el| {
                        el.cursor_pointer()
                            .hover(|el| el.bg(rgb(theme.active)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.cycle_share_relay(cx);
                            }))
                    })
                    .child(
                        svg()
                            .path("icons/globe.svg")
                            .size(px(12.))
                            .flex_none()
                            .text_color(rgb(theme.subtext())),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(crate::notifications::safe_text(&label, 80)),
                    ),
            );
        } else {
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .p(px(8.))
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.background))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("remote-share-relay-none")),
                    )
                    .child(
                        button(
                            theme,
                            "remote-share-relay-settings",
                            fl!("remote-share-open-settings"),
                            Tone::Plain,
                            true,
                        )
                        .flex_none()
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_menu(window, cx);
                            crate::settings_window::open_remote(cx.entity().downgrade(), cx);
                        })),
                    ),
            );
        }
        row
    }

    fn render_share_preview(
        &self,
        dialog: &ShareDialog,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut box_el = div()
            .debug_selector(|| "remote-share-preview".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(8.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.background))
            .border_1()
            .border_color(rgb(theme.active));

        match &dialog.preview {
            PreviewState::Loading => {
                box_el = box_el.child(
                    div()
                        .text_size(small)
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("remote-share-preview-loading")),
                );
            }
            PreviewState::Failed(error) => {
                box_el = box_el
                    .child(
                        div()
                            .text_size(small)
                            .text_color(crate::menu::danger(theme))
                            .child(crate::notifications::safe_text(error, 200)),
                    )
                    .child(
                        div().flex().justify_end().child(
                            button(
                                theme,
                                "remote-share-preview-retry",
                                fl!("remote-share-retry"),
                                Tone::Plain,
                                true,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.refresh_share_preview(cx);
                            })),
                        ),
                    );
            }
            PreviewState::Ready(preview) => {
                if preview.entries.is_empty() {
                    let empty = if dialog.history == ShareHistory::FromNow {
                        fl!("remote-share-preview-empty-from-now")
                    } else {
                        fl!("remote-share-preview-empty")
                    };
                    box_el = box_el.child(
                        div()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(empty),
                    );
                } else {
                    box_el = box_el
                        .child(
                            div()
                                .text_size(small)
                                .font_weight(FontWeight::MEDIUM)
                                .child(fl!(
                                    "remote-share-preview-count",
                                    count = preview.total_entries
                                )),
                        )
                        .child(
                            div()
                                .id("remote-share-preview-list")
                                .track_scroll(&dialog.preview_scroll)
                                .overflow_y_scroll()
                                .max_h(px(120.))
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .children(preview.entries.iter().map(|entry| {
                                    let role = match entry.role {
                                        PreviewRole::User => fl!("remote-share-preview-role-user"),
                                        PreviewRole::Agent => {
                                            fl!("remote-share-preview-role-agent")
                                        }
                                        PreviewRole::Tool => fl!("remote-share-preview-role-tool"),
                                    };
                                    div()
                                        .flex()
                                        .items_baseline()
                                        .gap(px(6.))
                                        .text_size(small)
                                        .child(
                                            div()
                                                .flex_none()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(theme.subtext()))
                                                .child(role),
                                        )
                                        .child(div().flex_1().min_w_0().truncate().child(
                                            crate::notifications::safe_text(&entry.text, 120),
                                        ))
                                })),
                        );
                }
                box_el = box_el.child(
                    div()
                        .text_size(small)
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("remote-share-preview-warning")),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-preview-label")),
            )
            .child(box_el)
    }

    fn render_share_created(
        &self,
        dialog: &ShareDialog,
        created: &sheprd_remote_control::control::sharing::ShareCreated,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let preview = crate::notifications::safe_text(created.link.expose(), 72);
        div()
            .debug_selector(|| "remote-share-created".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(8.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.background))
            .border_1()
            .border_color(crate::menu::accent(theme))
            .child(
                div()
                    .text_size(small)
                    .font_weight(FontWeight::MEDIUM)
                    .child(fl!("remote-share-created-label")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(preview),
                    )
                    .child(
                        button(
                            theme,
                            "remote-share-copy",
                            if dialog.copied {
                                fl!("remote-share-copied")
                            } else {
                                fl!("remote-share-copy")
                            },
                            Tone::Primary,
                            true,
                        )
                        .flex_none()
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.copy_share_link(cx);
                        })),
                    ),
            )
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-created-note")),
            )
    }

    fn render_share_active(
        &self,
        dialog: &ShareDialog,
        theme: &Theme,
        small: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let stopping = dialog.busy == BusyState::Stopping;
        div()
            .debug_selector(|| "remote-share-active".into())
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("remote-share-active-title")),
            )
            .children(
                dialog
                    .active
                    .iter()
                    .map(|entry| render_active_row(entry, theme, small, stopping, cx)),
            )
    }
}

fn render_active_row(
    entry: &ShareSummary,
    theme: &Theme,
    small: Pixels,
    stopping: bool,
    cx: &mut Context<SheprdWindow>,
) -> Div {
    let history = match entry.history {
        ShareHistory::FromNow => fl!("remote-share-history-from-now"),
        ShareHistory::All => fl!("remote-share-history-all"),
    };
    let tools = if entry.include_tools {
        fl!("remote-share-tools-on")
    } else {
        fl!("remote-share-tools-off")
    };
    let relay = entry
        .relay
        .strip_prefix("https://")
        .unwrap_or(&entry.relay)
        .to_owned();
    let share = entry.share;
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .px(px(8.))
        .py(px(6.))
        .rounded(px(corners::CONTROL))
        .bg(rgb(theme.background))
        .border_1()
        .border_color(rgb(theme.active))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(small)
                .child(fl!(
                    "remote-share-active-row",
                    history = history,
                    tools = tools,
                    relay = crate::notifications::safe_text(&relay, 48)
                )),
        )
        .child(
            div()
                .id(SharedString::from(format!("remote-share-stop-{share}")))
                .debug_selector(|| "remote-share-stop".into())
                .role(Role::Button)
                .flex_none()
                .px(px(10.))
                .py(px(4.))
                .rounded(px(corners::CONTROL))
                .bg(rgb(theme.active))
                .text_color(crate::menu::danger(theme))
                .when(!stopping, |btn| {
                    btn.cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.stop_share(share, cx);
                        }))
                })
                .when(stopping, |btn| btn.opacity(0.5))
                .child(if stopping {
                    fl!("remote-share-stopping")
                } else {
                    fl!("remote-share-stop")
                }),
        )
}

fn segment(theme: &Theme, id: &'static str, label: String, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .px(px(8.))
        .py(px(5.))
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(if selected {
            crate::menu::accent(theme)
        } else {
            rgb(theme.active)
        })
        .bg(if selected {
            rgb(theme.active)
        } else {
            rgb(theme.background)
        })
        .text_color(if selected {
            rgb(theme.foreground)
        } else {
            rgb(theme.subtext())
        })
        .when(selected, |el| el.font_weight(FontWeight::MEDIUM))
        .cursor_pointer()
        .hover(|el| el.bg(rgb(theme.active)))
        .child(label)
}
