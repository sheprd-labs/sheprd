use super::*;

impl SettingsWindow {
    fn show_providers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .voice
            .saved
            .as_ref()
            .and_then(|a| Provider::ALL.iter().position(|p| *p == a.provider))
            .unwrap_or(0);
        self.voice.visibility.set(true, crate::motion::now(), false);
        self.voice.sheet = Sheet::List(current);
        self.voice.notice.clear();
        self.dropdown = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(in crate::settings_window) fn voice_sheet_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let sheet = self.voice.sheet;
        if !self.voice.visibility.is_open() || self.dropdown.is_some() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            if !self.voice.busy {
                self.dismiss_voice_sheet(cx);
                window.focus(&self.focus, cx);
            }
        } else if let Sheet::List(index) = sheet {
            let count = Provider::ALL.len();
            match key {
                "up" => self.voice.sheet = Sheet::List((index + count - 1) % count),
                "down" => self.voice.sheet = Sheet::List((index + 1) % count),
                "tab" => {
                    self.voice.sheet = Sheet::List(
                        (index
                            + if event.keystroke.modifiers.shift {
                                count - 1
                            } else {
                                1
                            })
                            % count,
                    )
                }
                "enter" | "space" => self.select_provider(Provider::ALL[index], cx),
                _ => return false,
            }
        } else {
            return false;
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
        true
    }

    pub(in crate::settings_window) fn render_voice(&self, cx: &mut Context<Self>) -> Div {
        let active = self.voice.saved.as_ref().map_or_else(
            || fl!("settings-voice-disconnected"),
            |a| fl!("settings-voice-active", provider = a.provider.name()),
        );
        let idle = !self.voice.busy;
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                ui::row(
                    &self.theme,
                    "voice-provider",
                    fl!("settings-voice-provider"),
                )
                .control(div().child(active))
                .control(
                    ui::button(
                        &self.theme,
                        "voice-change",
                        fl!("settings-voice-change"),
                        idle,
                    )
                    .when(idle, |b| {
                        b.on_click(
                            cx.listener(|this, _, window, cx| this.show_providers(window, cx)),
                        )
                    }),
                ),
            )
            .when(
                !self.voice.visibility.is_open() && !self.voice.notice.is_empty(),
                |p| p.child(self.control_note(self.voice.notice.clone())),
            )
            .child(self.control_note(fl!("settings-voice-wispr")))
    }

    pub(in crate::settings_window) fn voice_sheet(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let toggle = self.voice.visibility;
        let now = crate::motion::now();
        if self.section != crate::settings_window::Section::Voice
            || !toggle.is_open() && (cfg!(test) || !toggle.moving(now))
        {
            return None;
        }
        if !cfg!(test) && toggle.moving(now) {
            window.request_animation_frame();
        }
        let shown = if cfg!(test) { 1. } else { toggle.shown(now) };
        let sheet = self.voice.sheet;
        let theme = &self.theme;
        let title = match sheet {
            Sheet::List(_) => fl!("settings-voice-providers"),
            Sheet::Account => self.voice.provider.name().to_owned(),
        };
        let idle = !self.voice.busy;
        let panel = div()
            .id("voice-sheet")
            .role(Role::Dialog)
            .aria_label(title.clone())
            .relative()
            .occlude()
            .w_full()
            .max_w(px(490.))
            .mb(px(-22. * (1. - shown)))
            .opacity(shown)
            .max_h_full()
            .overflow_y_scroll()
            .p(px(16.))
            .rounded_t(px(14.))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(12.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(16.))
                            .child(title),
                    )
                    .child(
                        ui::button(
                            theme,
                            "voice-sheet-close",
                            fl!("settings-voice-close"),
                            idle,
                        )
                        .when(idle, |b| {
                            b.on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_voice_sheet(cx);
                                window.focus(&this.focus, cx);
                            }))
                        }),
                    ),
            );
        let panel = match sheet {
            Sheet::Account => panel.child(self.voice_editor(cx)),
            Sheet::List(highlight) => {
                panel.children(Provider::ALL.into_iter().enumerate().map(|(i, provider)| {
                    let icon = match provider {
                        Provider::OpenAI => "codex",
                        Provider::ElevenLabs => "elevenlabs",
                        Provider::Deepgram => "deepgram",
                        Provider::OpenRouter => "openrouter",
                    };
                    let current = self
                        .voice
                        .saved
                        .as_ref()
                        .is_some_and(|a| a.provider == provider);
                    div()
                        .id(("voice-option", i))
                        .debug_selector(move || format!("voice-option-{i}"))
                        .role(Role::Button)
                        .aria_label(provider.name())
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .mb(px(3.))
                        .p(px(9.))
                        .rounded(px(8.))
                        .when(highlight == i, |b| b.bg(rgb(theme.active)))
                        .hover(|s| s.bg(rgb(theme.active)))
                        .child(
                            svg()
                                .path(format!("icons/providers/{icon}.svg"))
                                .size(px(24.))
                                .text_color(rgb(theme.foreground)),
                        )
                        .child(div().flex_1().child(provider.name()).child(
                            div().text_color(rgb(theme.subtext())).child(
                                if provider == Provider::OpenRouter {
                                    fl!("settings-voice-sign-in-or-key")
                                } else {
                                    fl!("settings-voice-api-key")
                                },
                            ),
                        ))
                        .child(if current {
                            fl!("settings-voice-in-use")
                        } else {
                            "›".into()
                        })
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_provider(provider, cx)),
                        )
                }))
            }
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .flex_col()
                .items_center()
                .justify_end()
                .child(
                    div()
                        .id("voice-sheet-scrim")
                        .absolute()
                        .inset_0()
                        .bg(rgb(0).opacity(0.34 * shown))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.voice.busy {
                                this.dismiss_voice_sheet(cx);
                                window.focus(&this.focus, cx);
                            }
                        })),
                )
                .child(panel),
        )
    }
}
