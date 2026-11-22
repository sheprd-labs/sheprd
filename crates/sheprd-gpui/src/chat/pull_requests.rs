//! `sheprd chat pr`: an agent in a Sheprd chat lists, or reads, the pull
//! requests that chat's rail shows. The chat program gives its agent its
//! own endpoint in [`sheprd_chat_protocol::AGENT_CHAT_ENV`], so the command reaches that chat
//! directly over its attach socket, as a command-line client, with or
//! without the app running. Nothing else is read or sent.

mod error;

pub(crate) use error::{Error, Result};

use sheprd_chat_protocol::{
    ClientKind, ClientMessage, Command, CommandId, EndpointId, PullRequest, REVISION,
    ServerMessage, VERSION, read_frame, write_frame,
};
use sheprd_client::private_endpoint::Connection;
use std::{
    ffi::OsStr,
    io::{BufReader, BufWriter, ErrorKind, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long the chat has to answer, from connecting to its last message.
const WAIT: Duration = Duration::from_secs(5);
/// Messages read past while waiting for the answer: a working chat streams
/// its changes to every client, this one included.
const MAX_MESSAGES: usize = 4096;

/// Where the chat the caller is the agent of listens, from
/// [`sheprd_chat_protocol::AGENT_CHAT_ENV`] as `value` gives it.
pub(crate) fn address(value: Option<&OsStr>) -> Result<PathBuf> {
    let value = value.ok_or(Error::NoChat)?;
    let chat: EndpointId = value
        .to_str()
        .and_then(|value| value.parse().ok())
        .ok_or(Error::InvalidEndpoint)?;
    sheprd_chat_protocol::endpoint_address(&chat).ok_or(Error::NoEndpointDir)
}

/// The pull requests the chat lists, oldest first.
pub(crate) fn list(chat: &Path) -> Result<Vec<PullRequest>> {
    let (mut reader, _writer) = connect(chat)?;
    let deadline = Instant::now() + WAIT;
    for _ in 0..MAX_MESSAGES {
        match next(&mut reader, deadline)? {
            ServerMessage::Snapshot { transcript, .. } => return Ok(transcript.pull_requests),
            ServerMessage::Bye { .. } => return Err(Error::Refused),
            _ => {}
        }
    }
    Err(Error::Timeout)
}

/// Adds the pull request at `url` to the chat's list; what it lists after.
pub(crate) fn add(chat: &Path, url: &str) -> Result<PullRequest> {
    let pull = PullRequest::parse(url.trim()).ok_or(Error::InvalidUrl)?;
    let (mut reader, mut writer) = connect(chat)?;
    let id = CommandId::random();
    let command = ClientMessage::Command {
        command: Command::AddPullRequest {
            url: pull.url.clone(),
        },
        id,
    };
    write_frame(&mut writer, &command)?;
    writer.flush()?;
    let deadline = Instant::now() + WAIT;
    for _ in 0..MAX_MESSAGES {
        match next(&mut reader, deadline)? {
            ServerMessage::Ack {
                id: acked,
                rejected,
            } if acked == id => {
                return match rejected {
                    Some(rejection) => Err(Error::Rejected(rejection)),
                    None => Ok(pull),
                };
            }
            ServerMessage::Bye { .. } => return Err(Error::Refused),
            _ => {}
        }
    }
    Err(Error::Timeout)
}

/// Connects and greets the chat as a command-line client.
fn connect(chat: &Path) -> Result<(impl Read, impl Write)> {
    let connection = Connection::connect(chat, WAIT).map_err(Error::Connect)?;
    let (mut reader, writer, _closer) = connection.split()?;
    reader.set_timeout(Some(WAIT))?;
    let mut writer = BufWriter::new(writer);
    let hello = ClientMessage::Hello {
        client: ClientKind::Cli,
        revision: REVISION,
        since: None,
        version: VERSION,
    };
    write_frame(&mut writer, &hello)?;
    writer.flush()?;
    Ok((BufReader::new(reader), writer))
}

/// The chat's next message, before `deadline`.
fn next(reader: &mut impl Read, deadline: Instant) -> Result<ServerMessage> {
    if Instant::now() >= deadline {
        return Err(Error::Timeout);
    }
    match read_frame(reader) {
        Ok(Some(message)) => Ok(message),
        Ok(None) => Err(Error::Closed),
        Err(sheprd_chat_protocol::Error::Io(error))
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
        {
            Err(Error::Timeout)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests;
