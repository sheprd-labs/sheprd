//! Machine failures, from `error-machine.ftl`.

use super::Localize;
use crate::{i18n::fl, machine::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            #[cfg(target_os = "macos")]
            Self::Agent(error) => error.localize(),
            Self::Busy(name) => fl!("error-machine-busy", name = name.as_str()),
            Self::Cpus { min, max } => fl!("error-machine-cpus", min = *min, max = *max),
            Self::Delete { path, source } => fl!(
                "error-machine-delete",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::Exists(name) => fl!("error-machine-exists", name = name.as_str()),
            Self::Framework(message) => {
                fl!("error-machine-framework", message = message.as_str())
            }
            Self::Hardware => fl!("error-machine-hardware"),
            Self::ImageNotFound(name) => {
                fl!("error-machine-image-not-found", name = name.as_str())
            }
            Self::InvalidDisplay => fl!("error-machine-invalid-display"),
            Self::InvalidName => fl!("error-machine-invalid-name"),
            Self::Memory { min, max } => fl!("error-machine-memory", min = *min, max = *max),
            Self::NoStateDir => fl!("error-machine-no-state-dir"),
            Self::NotInstalled(path) => fl!(
                "error-machine-not-installed",
                path = path.display().to_string()
            ),
            Self::Read { path, source } => fl!(
                "error-machine-read",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::Suspended(name) => fl!("error-machine-suspended", name = name.as_str()),
            Self::Unsupported => fl!("error-machine-unsupported"),
            Self::Write { path, source } => fl!(
                "error-machine-write",
                path = path.display().to_string(),
                error = source.to_string()
            ),
        }
    }
}

#[cfg(target_os = "macos")]
impl Localize for crate::machine::agent::Error {
    fn localize(&self) -> String {
        use crate::machine::agent::{Error, Refusal};
        match self {
            Error::Io(source) => fl!("error-machine-agent-io", error = source.to_string()),
            Error::NotRunning(name) => {
                fl!("error-machine-agent-not-running", name = name.as_str())
            }
            Error::Picture(_) => fl!("error-machine-agent-picture"),
            Error::Refused { refusal, message } => match refusal {
                Refusal::Accessibility => fl!("error-machine-agent-accessibility"),
                Refusal::ScreenRecording => fl!("error-machine-agent-screen-recording"),
                Refusal::Invalid | Refusal::Failed => {
                    fl!("error-machine-agent-refused", message = message.as_str())
                }
            },
            Error::Reply(_) => fl!("error-machine-agent-reply"),
            Error::TooLarge(bytes) => fl!("error-machine-agent-too-large", bytes = *bytes),
            Error::Unreachable { name, reason } => fl!(
                "error-machine-agent-unreachable",
                name = name.as_str(),
                reason = reason.as_str()
            ),
            Error::Write { path, source } => fl!(
                "error-machine-write",
                path = path.display().to_string(),
                error = source.to_string()
            ),
        }
    }
}
