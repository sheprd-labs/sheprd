//! Why a plugin action did not run, from `error-plugin-actions.ftl`.

use super::Localize;
use crate::{i18n::fl, plugin_actions::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::UnsupportedHost => fl!("error-plugin-actions-unsupported-host"),
            Self::DevelopmentSession => fl!("error-plugin-actions-development-session"),
            Self::MissingCli(_) => fl!("error-plugin-actions-missing-cli"),
            Self::Script(error) => fl!("error-plugin-actions-script", error = error.localize()),
            // Herdr's own message, as it wrote it.
            Self::Refused { message, .. } => {
                fl!("error-plugin-actions-refused", error = message.as_str())
            }
            Self::Decode(_) => fl!("error-plugin-actions-decode"),
            Self::Unanswered => fl!("error-plugin-actions-unanswered"),
            Self::TargetGone => fl!("error-plugin-actions-target-gone"),
        }
    }
}
