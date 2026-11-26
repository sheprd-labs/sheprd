//! Machine failures, from `error-machine.ftl`.

use super::Localize;
use crate::{i18n::fl, machine::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            #[cfg(target_os = "macos")]
            Self::Agent(error) => error.localize(),
            Self::Busy(name) => fl!("error-machine-busy", name = name.as_str()),
            Self::Capacity { in_use, limit } => {
                fl!("error-machine-capacity", in_use = *in_use, limit = *limit)
            }
            Self::Cpus { min, max } => fl!("error-machine-cpus", min = *min, max = *max),
            Self::Delete { path, source } => fl!(
                "error-machine-delete",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::Exists(name) => fl!("error-machine-exists", name = name.as_str()),
            #[cfg(target_os = "macos")]
            Self::Install(error) => error.localize(),
            Self::Framework(message) => {
                fl!("error-machine-framework", message = message.as_str())
            }
            Self::Hardware => fl!("error-machine-hardware"),
            Self::ImageNotFound(name) => {
                fl!("error-machine-image-not-found", name = name.as_str())
            }
            Self::InvalidDisplay => fl!("error-machine-invalid-display"),
            Self::InvalidMetadata { path, reason } => fl!(
                "error-machine-invalid-metadata",
                path = path.display().to_string(),
                reason = reason.as_str()
            ),
            Self::InvalidName => fl!("error-machine-invalid-name"),
            Self::Memory { min, max } => fl!("error-machine-memory", min = *min, max = *max),
            Self::NameMismatch {
                requested,
                installed,
            } => fl!(
                "error-machine-name-mismatch",
                requested = requested.as_str(),
                installed = installed.as_str()
            ),
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
            Self::Remote(error) => error.localize(),
            Self::Suspended(name) => fl!("error-machine-suspended", name = name.as_str()),
            Self::Unsupported => fl!("error-machine-unsupported"),
            Self::UnsupportedCapability { name, capability } => fl!(
                "error-machine-unsupported-capability",
                name = name.as_str(),
                capability = *capability
            ),
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
            Error::NotPng | Error::Picture(_) => fl!("error-machine-agent-picture"),
            Error::Refused { refusal, message } => match refusal {
                Refusal::Accessibility => fl!("error-machine-agent-accessibility"),
                Refusal::ScreenRecording => fl!("error-machine-agent-screen-recording"),
                Refusal::NotReady => {
                    fl!("error-machine-agent-not-ready", message = message.as_str())
                }
                Refusal::Unsupported => fl!(
                    "error-machine-agent-unsupported",
                    message = message.as_str()
                ),
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

#[cfg(target_os = "macos")]
impl Localize for crate::machine::install::Error {
    fn localize(&self) -> String {
        use crate::machine::install::Error;
        match self {
            Error::Cancelled => fl!("error-machine-install-cancelled"),
            Error::ChecksumMismatch { expected, actual } => fl!(
                "error-machine-install-checksum-mismatch",
                expected = expected.as_str(),
                actual = actual.as_str()
            ),
            Error::Download(source) => {
                fl!("error-machine-install-download", error = source.to_string())
            }
            Error::Fetch(message) => {
                fl!("error-machine-install-fetch", message = message.as_str())
            }
            Error::File { path, source } => fl!(
                "error-machine-write",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Error::ImageFormat(message) => {
                fl!(
                    "error-machine-install-image-format",
                    message = message.as_str()
                )
            }
            Error::Incomplete { got, expected } => fl!(
                "error-machine-install-incomplete",
                got = format!("{:.1}", *got as f64 / 1e9), // i18n: data
                expected = format!("{:.1}", *expected as f64 / 1e9)  // i18n: data
            ),
            Error::Install(message) => {
                fl!("error-machine-install-failed", message = message.as_str())
            }
            Error::NoRoom { needed, free } => fl!(
                "error-machine-install-no-room",
                needed = format!("{:.0}", *needed as f64 / 1e9), // i18n: data
                free = format!("{:.0}", *free as f64 / 1e9)      // i18n: data
            ),
            Error::NotSupported => fl!("error-machine-install-not-supported"),
            Error::Status(status) => fl!("error-machine-install-status", status = *status),
            Error::UnknownImage(image) => {
                fl!(
                    "error-machine-install-unknown-image",
                    image = image.as_str()
                )
            }
        }
    }
}

impl Localize for crate::machine::remote::Error {
    fn localize(&self) -> String {
        use crate::machine::remote::Error;
        match self {
            Error::Answer { host, source } => fl!(
                "error-machine-remote-answer",
                host = host.as_str(),
                error = source.to_string()
            ),
            Error::Base64(source) => {
                fl!("error-machine-remote-base64", error = source.to_string())
            }
            Error::Decode(source) => {
                fl!("error-machine-remote-decode", error = source.to_string())
            }
            Error::Encode(source) => {
                fl!("error-machine-remote-encode", error = source.to_string())
            }
            Error::Missing(host) => fl!("error-machine-remote-missing", host = host.as_str()),
            Error::NotPng => fl!("error-machine-remote-not-png"),
            Error::NotRunning(host) => {
                fl!("error-machine-remote-not-running", host = host.as_str())
            }
            Error::Refused { host, message } => fl!(
                "error-machine-remote-refused",
                host = host.as_str(),
                message = message.as_str()
            ),
            Error::Request { host, source } => fl!(
                "error-machine-remote-request",
                host = host.as_str(),
                error = source.to_string()
            ),
            Error::Run { host, source } => fl!(
                "error-machine-remote-run",
                host = host.as_str(),
                error = source.to_string()
            ),
            Error::TooLarge(host) => fl!("error-machine-remote-too-large", host = host.as_str()),
            Error::TooOld(host) => fl!("error-machine-remote-too-old", host = host.as_str()),
            Error::Write { path, source } => fl!(
                "error-machine-write",
                path = path.display().to_string(),
                error = source.to_string()
            ),
        }
    }
}
