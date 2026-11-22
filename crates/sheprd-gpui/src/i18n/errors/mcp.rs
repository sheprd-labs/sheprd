//! `sheprd mcp`: `error-mcp.ftl`.

use super::Localize;
use crate::{i18n::fl, mcp::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Choice { name, choices } => {
                fl!("error-mcp-choice", name = *name, choices = *choices)
            }
            Self::Executable => fl!("error-mcp-executable"),
            Self::Missing { name } => fl!("error-mcp-missing", name = *name),
            Self::Off => fl!("error-mcp-off"),
            Self::Range { name, min, max } => fl!(
                "error-mcp-range",
                name = *name,
                min = min.to_string(),
                max = max.to_string()
            ),
            // The server's own words.
            Self::Serve(error) => fl!("error-mcp-serve", error = error.to_string()),
            Self::ServeOnly => fl!("error-mcp-serve-only"),
            Self::TooLong { name, max } => {
                fl!("error-mcp-too-long", name = *name, max = max.to_string())
            }
            Self::Type { name, expected } => {
                fl!("error-mcp-type", name = *name, expected = *expected)
            }
            Self::Unknown { name } => fl!("error-mcp-unknown", name = name.clone()),
            // keep-sorted end
        }
    }
}
