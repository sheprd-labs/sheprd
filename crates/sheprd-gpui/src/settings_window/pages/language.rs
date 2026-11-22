//! General > Language: the `language` setting, offered as System and the
//! languages that can be selected directly, each named in itself.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::{
    i18n::{self, fl},
    settings_window::inputs::DropdownField,
};
use gpui::{prelude::*, *};
use sheprd_i18n::{Language, LanguageIdentifier, Locale};

/// System, the selectable languages, and an explicitly configured tag
/// when it isn't already listed, so the picker reflects the setting.
pub(in crate::settings_window) fn language_choices(configured: &Language) -> Vec<Language> {
    let mut choices: Vec<_> = std::iter::once(Language::System)
        .chain(i18n::selectable().iter().cloned().map(Language::Locale))
        .collect();
    if !choices.contains(configured) {
        choices.push(configured.clone());
    }
    choices
}

/// A choice as the picker names it: a language by its endonym, System and
/// an unavailable tag by what they show, and a pseudo-locale by its tag.
fn label(language: &Language, selectable: &[Locale], system: &[LanguageIdentifier]) -> String {
    let shown = || i18n::endonym(&i18n::resolve(language, system));
    match language {
        Language::System => fl!("settings-general-language-system", language = shown()),
        Language::Locale(locale) if selectable.contains(locale) => i18n::endonym(locale),
        Language::Locale(locale) if locale.pseudo().is_some() => locale.to_string(),
        Language::Locale(locale) => fl!(
            "settings-general-language-unavailable",
            tag = locale.to_string(),
            language = shown()
        ),
    }
}

impl SettingsWindow {
    pub(in crate::settings_window) fn language_row(&self, cx: &mut Context<Self>) -> ui::Row {
        const FIELD: DropdownField = DropdownField::Language;
        let configured = &self.config.language;
        let selectable = i18n::selectable();
        let system = sheprd_i18n::system_languages();
        let choices = language_choices(configured);
        let selected = choices
            .iter()
            .position(|choice| choice == configured)
            .unwrap_or(0);
        let options: Vec<SharedString> = choices
            .iter()
            .map(|choice| SharedString::from(label(choice, selectable, &system)))
            .collect();
        let current = options.get(selected).cloned().unwrap_or_default();
        let view = cx.entity().downgrade();
        let (opener, picker, dismisser) = (view.clone(), view.clone(), view);
        ui::row(
            &self.theme,
            "general-language",
            fl!("settings-general-language"),
        )
        .description(fl!("settings-general-language-description"))
        .control(ui::dropdown(
            &self.theme,
            FIELD.selector(),
            current,
            false,
            !self.quitting && self.closing.is_none(),
            self.dropdown.as_ref().filter(|open| open.field == FIELD),
            move |window, cx| {
                let options = options.clone();
                let _ = opener.update(cx, |this, cx| {
                    this.open_dropdown(FIELD, options, selected, window, cx)
                });
            },
            move |index, window, cx| {
                let _ = picker.update(cx, |this, cx| {
                    this.pick_dropdown(FIELD, index, cx);
                    window.focus(&this.focus, cx);
                });
            },
            move |window, cx| {
                let _ = dismisser.update(cx, |this, cx| this.close_dropdown(window, cx));
            },
        ))
    }
}
