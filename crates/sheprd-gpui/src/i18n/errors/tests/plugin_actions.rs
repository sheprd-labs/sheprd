use super::*;
use crate::plugin_actions::Error;
use sheprd_i18n::{Locale, Pseudo};

fn every_variant() -> Vec<Error> {
    vec![
        Error::UnsupportedHost,
        Error::DevelopmentSession,
        Error::MissingCli(sheprd_client::Error::ResponseType),
        Error::Script(sheprd_client::Error::ScriptOutputLimit),
        Error::Refused {
            code: "plugin_disabled".into(),
            message: "plugin hunk is disabled".into(),
        },
        Error::Decode(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::Unanswered,
        Error::TargetGone,
    ]
}

#[test]
fn every_plugin_action_variant_has_a_message() {
    assert_localized(&every_variant());
}

/// The app's own error shows the feature's message, in every language.
#[test]
fn the_app_error_shows_the_plugin_action_message() {
    let wrapped: Vec<crate::Error> = every_variant().into_iter().map(Into::into).collect();
    assert_localized(&wrapped);
    for (error, inner) in wrapped.iter().zip(every_variant()) {
        assert_eq!(error.to_string(), inner.to_string());
    }
}

/// Herdr's refusal is shown as Herdr wrote it, untranslated.
#[test]
fn herdrs_refusal_keeps_its_words() {
    let refused = Error::Refused {
        code: "action_not_found".into(),
        message: "action review-pane not found in plugin hunk".into(),
    };
    crate::i18n::show(&Pseudo::Accented.locale());
    let shown = refused.localize();
    crate::i18n::show(&Locale::FALLBACK);
    assert!(
        shown.contains("action review-pane not found in plugin hunk"),
        "{shown}"
    );
}
