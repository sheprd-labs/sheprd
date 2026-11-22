//! `sheprd chat pr` against the caller's chat: `error-chat-pull-requests.ftl`.

use super::Localize;
use crate::{chat::pull_requests::Error, i18n::fl};
use sheprd_chat_protocol::AGENT_CHAT_ENV;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Closed => fl!("error-chat-pull-requests-closed"),
            Self::Connect(_) => fl!("error-chat-pull-requests-connect"),
            Self::InvalidEndpoint => fl!(
                "error-chat-pull-requests-invalid-endpoint",
                variable = AGENT_CHAT_ENV
            ),
            Self::InvalidUrl => fl!("error-chat-pull-requests-invalid-url"),
            Self::Io(_) => fl!("error-chat-pull-requests-io"),
            Self::NoChat => fl!(
                "error-chat-pull-requests-no-chat",
                variable = AGENT_CHAT_ENV
            ),
            Self::NoEndpointDir => fl!("error-chat-pull-requests-no-endpoint-dir"),
            Self::Protocol(_) => fl!("error-chat-pull-requests-protocol"),
            Self::Refused => fl!("error-chat-pull-requests-refused"),
            Self::Rejected(rejection) => fl!(
                "error-chat-pull-requests-rejected",
                reason = rejection.localize()
            ),
            Self::Timeout => fl!("error-chat-pull-requests-timeout"),
            // keep-sorted end
        }
    }
}
