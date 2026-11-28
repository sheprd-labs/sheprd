use crate::wire::Status;

/// Why reaching a relay over its WebSocket API, or a step on it, failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("this relay has banned your network address")]
    Banned,
    #[error("the relay closed the socket (code {code:?})")]
    Closed { code: Option<u16> },
    #[error("could not reach the relay")]
    Connect(#[source] std::io::Error),
    #[error(transparent)]
    Framing(#[from] crate::framing::Error),
    #[error("the relay's address has no host to connect to")]
    Host,
    #[error("the relay answered HTTP {status}")]
    Http { status: u16 },
    #[error("the session failed")]
    Io(#[source] std::io::Error),
    #[error("a pairing key lives 1 to {max} seconds, not {seconds}")]
    Lifetime { max: u16, seconds: u16 },
    #[error("the relay sent something that is not its protocol")]
    Malformed,
    #[error("the relay answered {0:?}")]
    Refused(Status),
    #[error("another listener with the same key took this route")]
    Replaced,
    #[error("the listener has stopped")]
    Stopped,
    #[error("the relay did not answer in time")]
    Timeout,
    #[error("could not set up TLS")]
    Tls(#[source] rustls::Error),
    #[error("the relay's host is not a name TLS can check")]
    TlsName(#[source] rustls::pki_types::InvalidDnsNameError),
    #[error("a listener names at most {max} dial keys, not {count}")]
    TooManyKeys { count: usize, max: usize },
    #[error("the session ended in the middle of a framed message")]
    Truncated,
    #[error("the WebSocket failed")]
    WebSocket(#[source] Box<tokio_tungstenite::tungstenite::Error>),
    #[error("the relay said it listens on another route")]
    WrongRoute,
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
