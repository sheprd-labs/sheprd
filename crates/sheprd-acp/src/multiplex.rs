//! `sheprd-acp multiplex`: carries many client connections to this host's
//! chats over standard input and output at once, so Sheprd attaches every
//! chat on an SSH host through one channel rather than running `connect`
//! once per chat. Each stream the client opens names a chat's endpoint id
//! and is connected to it exactly as `connect` would: under this user's own
//! private directory, with its bytes passed through unread. The framing,
//! its limits, and its back-pressure are `sheprd_chat_protocol::mux`.

use std::{
    io::{self, Read, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

use sheprd_chat_protocol::{
    EndpointId,
    mux::{CloseReason, MAX_STREAMS, Mux, Stream},
};
use sheprd_client::private_endpoint::Connection;

use crate::Result;

/// How long connecting waits for a free pipe instance on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn run() -> Result<()> {
    serve(
        io::stdin(),
        io::stdout(),
        sheprd_chat_protocol::endpoint_address,
    )
}

/// Greets on `output`, then connects each stream opened on `input` to the
/// chat `address` finds for it, until the client goes away.
fn serve(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
    address: impl Fn(&EndpointId) -> Option<PathBuf>,
) -> Result<()> {
    let mux = Mux::host(MAX_STREAMS);
    let sender = mux.clone();
    thread::Builder::new()
        .name("sheprd-acp-mux-out".into())
        .spawn(move || {
            let _ = sender.send_loop(output);
            sender.shutdown();
        })?;
    let receiver = mux.clone();
    thread::Builder::new()
        .name("sheprd-acp-mux-in".into())
        .spawn(move || receiver.receive_loop(input))?;
    // Returning ends the process, and with it every stream's threads and
    // sockets: the chats themselves only lose a client.
    while let Some((endpoint, stream)) = mux.accept() {
        let address = address(&endpoint);
        let closing = stream.clone();
        let spawned = thread::Builder::new()
            .name("sheprd-acp-mux-stream".into())
            .spawn(move || carry(address, stream));
        if spawned.is_err() {
            closing.close_with(CloseReason::Unreachable);
        }
    }
    Ok(())
}

/// Connects `stream` to the chat at `address` and copies both ways until
/// either side ends.
fn carry(address: Option<PathBuf>, stream: Stream) {
    let parts = address
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        .and_then(|address| Connection::connect(&address, CONNECT_TIMEOUT))
        .and_then(Connection::split)
        .and_then(|(reader, writer, closer)| Ok((reader, writer, closer.try_clone()?, closer)));
    let Ok((reader, writer, upstream_closer, closer)) = parts else {
        stream.close_with(CloseReason::Unreachable);
        return;
    };
    let upstream = stream.clone();
    let spawned = thread::Builder::new()
        .name("sheprd-acp-mux-up".into())
        .spawn(move || {
            let _ = crate::relay::pump(upstream, writer);
            upstream_closer.close();
        });
    if spawned.is_ok() {
        let _ = crate::relay::pump(reader, stream.clone());
    }
    stream.close();
    closer.close();
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
