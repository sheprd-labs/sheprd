//! A Windows host's PowerShell failures, from `error-windows-host.ftl`.

use super::Localize;
use crate::{i18n::fl, windows_host::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Closed => fl!("error-windows-host-closed"),
            Self::Size => fl!("error-windows-host-size"),
            Self::Start(_) => fl!("error-windows-host-start"),
            Self::Timeout => fl!("error-windows-host-timeout"),
        }
    }
}
