mod chooser;

use super::{SettingsWindow, ui};
use crate::{
    i18n::fl,
    search_input::SearchInput,
    voice::{self, Account, Provider},
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

pub(in crate::settings_window) struct VoicePage {
    provider: Provider,
    saved: Option<Account>,
    sheet: Sheet,
    visibility: crate::motion::Toggle,
    key: Entity<SearchInput>,
    voice: Entity<SearchInput>,
    verifier: Option<SecretString>,
    choices: Vec<voice::Choice>,
    notice: String,
    busy: bool,
    previewing: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Sheet {
    List(usize),
    Account,
}

impl VoicePage {
    pub(in crate::settings_window) fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let key = cx.new(|cx| {
            let mut key = SearchInput::new(cx);
            key.set_masked(true, cx);
            key.set_placeholder(&fl!("settings-voice-api-key"), cx);
            key
        });
        Self {
            provider: Provider::OpenAI,
            saved: None,
            sheet: Sheet::List(0),
            visibility: Default::default(),
            key,
            voice: cx.new(SearchInput::new),
            verifier: None,
            choices: voice::choices(Provider::OpenAI),
            notice: String::new(),
            busy: false,
            previewing: false,
        }
    }
}

impl SettingsWindow {
    fn select_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.voice.provider = provider;
        self.voice.choices = voice::choices(provider);
        self.voice.verifier = None;
        self.voice.key.update(cx, |field, cx| {
            field.clear(cx);
            field.set_placeholder(&fl!("settings-voice-api-key"), cx);
        });
        self.voice.voice.update(cx, |field, cx| {
            field.set_label(&fl!("settings-voice-id"), cx)
        });
        let id = provider.default_voice();
        self.voice
            .voice
            .update(cx, |field, cx| field.set_text_at(id, id.len(), cx));
        if let Some(saved) = &self.voice.saved
            && saved.provider == provider
        {
            self.voice.key.update(cx, |f, cx| {
                f.set_text_at(saved.key.expose(), saved.key.expose().len(), cx)
            });
            self.voice.voice.update(cx, |f, cx| {
                f.set_text_at(&saved.voice, saved.voice.len(), cx)
            });
        }
        self.voice.notice.clear();
        self.voice.sheet = Sheet::Account;
        cx.notify();
    }

    pub(in crate::settings_window) fn dismiss_voice_sheet(&mut self, cx: &mut Context<Self>) {
        self.voice
            .visibility
            .set(false, crate::motion::now(), false);
        self.voice.verifier = None;
        self.voice.key.update(cx, |field, cx| field.clear(cx));
        self.dropdown = None;
        cx.notify();
    }

    pub(in crate::settings_window) fn open_voice(&mut self, cx: &mut Context<Self>) {
        if self.voice.busy {
            return;
        }
        self.voice.busy = true;
        let job = cx.background_executor().spawn(async { voice::load() });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.voice.busy = false;
                match result {
                    Ok(saved) => {
                        this.voice.saved = saved;
                        this.voice.notice.clear();
                    }
                    Err(e) => this.voice.notice = e.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn voice_action(&mut self, action: ActionKind, cx: &mut Context<Self>) {
        let mut account = Account {
            provider: self.voice.provider,
            voice: self.voice.voice.read(cx).text().trim().to_owned(),
            key: SecretString::from(self.voice.key.read(cx).text().trim()).into(),
        };
        let verifier = self.voice.verifier.clone();
        self.voice.busy = true;
        self.voice.notice.clear();
        cx.notify();
        self.voice.previewing = action == ActionKind::Preview;
        let session = if self.voice.previewing {
            voice::Session::begin(voice::Kind::Preview)
        } else {
            None
        };
        let job = cx.background_executor().spawn(async move {
            if action == ActionKind::Disconnect {
                voice::save(None)?;
                return Ok(None);
            }
            if !account.valid() {
                return Err(voice::Error::Invalid);
            }
            if action == ActionKind::Authorize {
                let verifier = verifier.ok_or(voice::Error::Invalid)?;
                account.key = voice::exchange(&verifier, account.key.expose())?;
                if !account.valid() {
                    return Err(voice::Error::Invalid);
                }
            }
            match action {
                ActionKind::Preview => {
                    let session = session.as_ref().ok_or(voice::Error::Invalid)?;
                    let result = voice::speak(&account, voice::SAMPLE, session);
                    if session.cancelled() {
                        return Err(voice::Error::Stopped);
                    }
                    result?;
                }
                ActionKind::Save => voice::save(Some(&account))?,
                ActionKind::Authorize | ActionKind::Disconnect => {}
            }
            Ok(Some(account))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.voice.busy = false;
                this.voice.previewing = false;
                match result {
                    Ok(Some(account)) => {
                        this.voice.verifier = None;
                        this.voice.key.update(cx, |field, cx| {
                            field.set_placeholder(&fl!("settings-voice-api-key"), cx);
                            field.set_text_at(account.key.expose(), account.key.expose().len(), cx);
                        });
                        this.voice.notice = if action == ActionKind::Save {
                            this.voice.saved = Some(account);
                            this.dismiss_voice_sheet(cx);
                            String::new()
                        } else if action == ActionKind::Authorize {
                            fl!("settings-voice-authorized")
                        } else {
                            fl!("settings-voice-previewed")
                        };
                    }
                    Ok(None) => {
                        this.voice.saved = None;
                        this.dismiss_voice_sheet(cx);
                        this.voice.notice.clear();
                    }
                    Err(error) => this.voice.notice = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_voices(&mut self, cx: &mut Context<Self>) {
        let key = SecretString::from(self.voice.key.read(cx).text().trim()).into();
        self.voice.busy = true;
        let job = cx
            .background_executor()
            .spawn(async move { voice::elevenlabs_voices(&key) });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.voice.busy = false;
                match result {
                    Ok(choices) => this.voice.choices = choices,
                    Err(error) => this.voice.notice = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::settings_window) fn pick_voice(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(choice) = self.voice.choices.get(index) {
            self.voice.voice.update(cx, |field, cx| {
                field.set_text_at(&choice.voice_id, choice.voice_id.len(), cx)
            });
        }
        cx.notify();
    }

    fn voice_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        use crate::settings_window::inputs::DropdownField;
        let field = DropdownField::SpeechVoice;
        let selected = self
            .voice
            .choices
            .iter()
            .position(|c| c.voice_id == self.voice.voice.read(cx).text())
            .unwrap_or(0);
        let options: Vec<SharedString> = self
            .voice
            .choices
            .iter()
            .map(|choice| choice.name.clone().into())
            .collect();
        let view = cx.entity().downgrade();
        let (opener, picker) = (view.clone(), view.clone());
        ui::dropdown(
            &self.theme,
            field.selector(),
            fl!("settings-voice-choose"),
            false,
            !self.voice.busy && !options.is_empty(),
            self.dropdown.as_ref().filter(|d| d.field == field),
            move |window, cx| {
                let _ = opener.update(cx, |this, cx| {
                    this.open_dropdown(field, options.clone(), selected, window, cx)
                });
            },
            move |index, window, cx| {
                let _ = picker.update(cx, |this, cx| {
                    this.pick_dropdown(field, index, cx);
                    window.focus(&this.focus, cx);
                });
            },
            move |window, cx| {
                let _ = view.update(cx, |this, cx| this.close_dropdown(window, cx));
            },
        )
    }

    fn voice_editor(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let idle = !self.voice.busy;
        let action = |id: &'static str, label: String, kind, cx: &mut Context<Self>| {
            if kind == ActionKind::Preview && self.voice.previewing {
                return ui::button(theme, id, fl!("settings-voice-stop"), true)
                    .on_click(|_, _, _| voice::Session::stop_preview());
            }
            let enabled = idle
                && (kind != ActionKind::Disconnect
                    || self
                        .voice
                        .saved
                        .as_ref()
                        .is_some_and(|a| a.provider == self.voice.provider))
                && (self.voice.verifier.is_none()
                    || matches!(kind, ActionKind::Authorize | ActionKind::Disconnect));
            ui::button(theme, id, label, enabled).when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| this.voice_action(kind, cx)))
            })
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(self.voice.provider == Provider::OpenRouter, |page| {
                page.child(
                    ui::button(theme, "voice-sign-in", fl!("settings-voice-sign-in"), idle).when(
                        idle,
                        |button| {
                            button.on_click(cx.listener(|this, _, _, cx| {
                                match voice::sign_in() {
                                    Ok((verifier, url)) => {
                                        this.voice.verifier = Some(verifier);
                                        this.voice.key.update(cx, |field, cx| {
                                            field.clear(cx);
                                            field.set_placeholder(&fl!("settings-voice-code"), cx);
                                        });
                                        cx.open_url(&url);
                                    }
                                    Err(e) => this.voice.notice = e.to_string(),
                                }
                                cx.notify();
                            }))
                        },
                    ),
                )
            })
            .child(
                ui::row(
                    theme,
                    "voice-key",
                    if self.voice.verifier.is_some() {
                        fl!("settings-voice-code")
                    } else {
                        fl!("settings-voice-api-key")
                    },
                )
                .control(div().w(px(220.)).child(self.voice.key.clone())),
            )
            .when(self.voice.verifier.is_some(), |page| {
                page.child(action(
                    "voice-authorize",
                    fl!("settings-voice-authorize"),
                    ActionKind::Authorize,
                    cx,
                ))
            })
            .child(
                ui::row(theme, "voice-id", fl!("settings-voice-id"))
                    .control(div().w(px(200.)).child(self.voice.voice.clone()))
                    .control(self.voice_picker(cx)),
            )
            .when(self.voice.provider == Provider::ElevenLabs, |page| {
                page.child(
                    ui::button(theme, "voice-refresh", fl!("settings-voice-refresh"), idle).when(
                        idle,
                        |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.load_voices(cx)))
                        },
                    ),
                )
            })
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(action(
                        "voice-preview",
                        fl!("settings-voice-preview"),
                        ActionKind::Preview,
                        cx,
                    ))
                    .child(action(
                        "voice-save",
                        fl!("settings-voice-save"),
                        ActionKind::Save,
                        cx,
                    ))
                    .child(action(
                        "voice-disconnect",
                        fl!("settings-voice-disconnect"),
                        ActionKind::Disconnect,
                        cx,
                    )),
            )
            .child(self.control_note(self.voice.notice.clone()))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ActionKind {
    Authorize,
    Preview,
    Save,
    Disconnect,
}
