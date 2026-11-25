//! Opening a socket to the relay, `ws` or `wss`, and the challenge both
//! listeners and dialers answer.

use super::{Config, Error, Result};
use crate::{
    endpoint::is_ban,
    socket::{Hello, NONCE_LEN, ToPeer},
    wire::KEY_LEN,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use rustls::{ClientConfig, pki_types::ServerName};
use rustls_platform_verifier::BuilderVerifierExt;
use std::{
    io,
    pin::Pin,
    sync::{Arc, OnceLock},
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    time::timeout,
};
use tokio_rustls::{TlsConnector, client::TlsStream};
use tokio_tungstenite::{
    WebSocketStream, client_async_with_config,
    tungstenite::{self, Message, client::IntoClientRequest, protocol::WebSocketConfig},
};
use url::{Host, Url};

/// The largest frame or message read from the relay: its own largest
/// `max-frame`.
const READ_LIMIT: usize = 16 * 1024 * 1024;

/// A socket to the relay.
pub(crate) type Socket = WebSocketStream<Stream>;

/// A TCP connection, encrypted for `wss`.
#[derive(Debug)]
pub enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

/// TLS as the platform checks certificates, made once.
fn tls() -> Result<Arc<ClientConfig>> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(Arc::clone(config));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(Error::Tls)?
        .with_platform_verifier()
        .map_err(Error::Tls)?
        .with_no_client_auth();
    Ok(Arc::clone(CONFIG.get_or_init(|| Arc::new(config))))
}

/// Opens `url` (`ws` or `wss`) within the config's handshake time. The
/// request carries no `Origin`: the relay refuses browsers on other sites,
/// and this is not one.
pub(crate) async fn open(url: &Url, config: &Config) -> Result<Socket> {
    timeout(config.handshake_timeout, open_now(url))
        .await
        .map_err(|_| Error::Timeout)?
}

async fn open_now(url: &Url) -> Result<Socket> {
    let host = match url.host() {
        Some(Host::Domain(name)) => name.to_owned(),
        Some(Host::Ipv4(address)) => address.to_string(),
        Some(Host::Ipv6(address)) => address.to_string(),
        None => return Err(Error::Host),
    };
    let port = url.port_or_known_default().ok_or(Error::Host)?;
    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(Error::Connect)?;
    // Small writes are interactive; Nagle would hold them back.
    let _ = tcp.set_nodelay(true);
    let stream = if url.scheme() == "wss" {
        let name = ServerName::try_from(host).map_err(Error::TlsName)?;
        let tls = TlsConnector::from(tls()?)
            .connect(name, tcp)
            .await
            .map_err(Error::Connect)?;
        Stream::Tls(Box::new(tls))
    } else {
        Stream::Plain(tcp)
    };
    let request = url.as_str().into_client_request().map_err(websocket)?;
    let limits = WebSocketConfig::default()
        .max_message_size(Some(READ_LIMIT))
        .max_frame_size(Some(READ_LIMIT));
    match client_async_with_config(request, stream, Some(limits)).await {
        Ok((socket, _)) => Ok(socket),
        Err(tungstenite::Error::Http(response)) => {
            let headers = response.headers();
            let banned = is_ban(|name| headers.get(name).and_then(|value| value.to_str().ok()));
            match response.status().as_u16() {
                403 if banned => Err(Error::Banned),
                status => Err(Error::Http { status }),
            }
        }
        Err(error) => Err(websocket(error)),
    }
}

pub(crate) fn websocket(error: tungstenite::Error) -> Error {
    Error::WebSocket(Box::new(error))
}

/// The next control message, skipping pings; a close or binary frame
/// before it is an error.
pub(crate) async fn control(socket: &mut Socket) -> Result<ToPeer> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(|_| Error::Malformed);
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
            Some(Ok(Message::Binary(_))) => return Err(Error::Malformed),
            Some(Ok(Message::Close(frame))) => {
                return Err(closed(frame.map(|frame| frame.code.into())));
            }
            Some(Err(error)) => return Err(websocket(error)),
            None => return Err(Error::Closed { code: None }),
        }
    }
}

/// What a close with `code` means to a client.
pub(crate) fn closed(code: Option<u16>) -> Error {
    match code {
        Some(crate::socket::code::REPLACED) => Error::Replaced,
        Some(crate::socket::code::BANNED) => Error::Banned,
        code => Error::Closed { code },
    }
}

/// Sends `value` as a text frame.
pub(crate) async fn send(socket: &mut Socket, value: &impl serde::Serialize) -> Result<()> {
    let text = serde_json::to_string(value).map_err(|_| Error::Malformed)?;
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(websocket)
}

/// Reads the relay's challenge and answers it with `key`'s signature over
/// what `signed` makes of the nonce.
pub(crate) async fn prove(
    socket: &mut Socket,
    key: &SigningKey,
    token: Option<&str>,
    signed: impl FnOnce(&[u8; NONCE_LEN]) -> Vec<u8>,
) -> Result<()> {
    let ToPeer::Challenge { nonce, version } = control(socket).await? else {
        return Err(Error::Malformed);
    };
    if version != crate::endpoint::PROTOCOL {
        return Err(Error::Malformed);
    }
    let nonce: [u8; NONCE_LEN] = URL_SAFE_NO_PAD
        .decode(nonce)
        .ok()
        .and_then(|nonce| nonce.try_into().ok())
        .ok_or(Error::Malformed)?;
    let signature: Signature = key.sign(&signed(&nonce));
    let hello = Hello {
        key: URL_SAFE_NO_PAD.encode(public(key)),
        signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        token: token.map(str::to_owned),
    };
    send(socket, &hello).await
}

/// `key`'s public half as the protocol carries it: SEC1, compressed.
#[must_use]
pub fn public(key: &SigningKey) -> crate::wire::Key {
    let mut public = [0; KEY_LEN];
    public.copy_from_slice(key.verifying_key().to_sec1_point(true).as_bytes());
    public
}
