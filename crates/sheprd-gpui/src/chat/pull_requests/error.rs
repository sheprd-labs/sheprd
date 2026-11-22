use sheprd_chat_protocol::{AGENT_CHAT_ENV, Rejection};
use std::io;

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the chat ended the connection before it answered")]
    Closed,
    #[error("the chat is not running or cannot be reached")]
    Connect(#[source] io::Error),
    #[error("{AGENT_CHAT_ENV} does not name a chat")]
    InvalidEndpoint,
    #[error("not a GitHub pull request address, such as https://github.com/OWNER/REPO/pull/7")]
    InvalidUrl,
    #[error("talking to the chat failed")]
    Io(#[from] io::Error),
    #[error("{AGENT_CHAT_ENV} is not set: run this as the agent of a Sheprd chat")]
    NoChat,
    #[error("neither XDG_STATE_HOME nor HOME is set, so the chat cannot be found")]
    NoEndpointDir,
    #[error("the chat sent a message this version cannot read")]
    Protocol(#[from] sheprd_chat_protocol::Error),
    #[error("the chat refused the connection")]
    Refused,
    #[error("the chat refused: {0}")]
    Rejected(Rejection),
    #[error("the chat did not answer in time")]
    Timeout,
}
