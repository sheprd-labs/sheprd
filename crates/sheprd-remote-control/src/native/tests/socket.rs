//! The live socket over a real connection: only a signed-in device
//! upgrades, it reads text frames of bounded size, and a binary or
//! oversized frame closes it.

use super::*;
use futures_lite::FutureExt;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{self, Message as Frame, client::IntoClientRequest},
};

type Socket = WebSocketStream<TcpStream>;

/// Serves the router on a loopback port; its address.
async fn serve(state: Shared) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, crate::web::router(state))
            .await
            .unwrap();
    });
    address
}

async fn connect(
    address: std::net::SocketAddr,
    token: Option<&str>,
) -> Result<Socket, tungstenite::Error> {
    let mut request = format!("ws://{address}/api/v2/live")
        .into_client_request()
        .unwrap();
    if let Some(token) = token {
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    let stream = TcpStream::connect(address).await.unwrap();
    client_async(request, stream)
        .await
        .map(|(socket, _)| socket)
}

/// The next text frame as a server message, skipping pings.
async fn next(socket: &mut Socket) -> Option<ServerMessage> {
    let read = async {
        loop {
            match socket.next().await? {
                Ok(Frame::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
                Ok(Frame::Ping(_) | Frame::Pong(_)) => {}
                Ok(_) | Err(_) => return None,
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), read)
        .await
        .expect("no frame in time")
}

/// Whether the server ends the connection within five seconds.
async fn closes(socket: &mut Socket) -> bool {
    let closed = async {
        loop {
            match socket.next().await {
                None | Some(Err(_) | Ok(Frame::Close(_))) => return true,
                Some(Ok(_)) => {}
            }
        }
    };
    closed
        .or(async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            false
        })
        .await
}

fn status(error: tungstenite::Error) -> u16 {
    match error {
        tungstenite::Error::Http(response) => response.status().as_u16(),
        other => panic!("{other}"),
    }
}

async fn send(socket: &mut Socket, text: &str) {
    socket.send(Frame::Text(text.into())).await.unwrap();
}

#[tokio::test]
async fn only_a_signed_in_device_upgrades() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let address = serve(state).await;
    assert_eq!(status(connect(address, None).await.unwrap_err()), 401);
    assert_eq!(
        status(connect(address, Some("forged")).await.unwrap_err()),
        401
    );
}

#[tokio::test]
async fn a_subscription_answers_and_a_bad_message_is_answered_with_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (_, token) = crate::web::tests::signed_in(&state, 9);
    let address = serve(state).await;
    let mut socket = connect(address, Some(&token)).await.unwrap();
    send(&mut socket, r#"{"type":"subscribe","view":"home"}"#).await;
    assert!(matches!(
        next(&mut socket).await,
        Some(ServerMessage::Home { .. })
    ));
    send(&mut socket, "not json").await;
    assert!(matches!(
        next(&mut socket).await,
        Some(ServerMessage::Error { .. })
    ));
}

#[tokio::test]
async fn a_binary_frame_closes_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (_, token) = crate::web::tests::signed_in(&state, 9);
    let address = serve(state).await;
    let mut socket = connect(address, Some(&token)).await.unwrap();
    socket
        .send(Frame::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert!(closes(&mut socket).await);
}

#[tokio::test]
async fn an_oversized_message_closes_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (_, token) = crate::web::tests::signed_in(&state, 9);
    let address = serve(state).await;
    let mut socket = connect(address, Some(&token)).await.unwrap();
    let padding = "x".repeat(MAX_INBOUND + 1);
    send(
        &mut socket,
        &format!(r#"{{"type":"subscribe","view":"home","pad":"{padding}"}}"#),
    )
    .await;
    assert!(closes(&mut socket).await);
}

#[tokio::test]
async fn a_device_with_too_many_streams_open_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (device, token) = crate::web::tests::signed_in(&state, 9);
    state.connected().insert(device, MAX_STREAMS);
    let address = serve(state).await;
    assert_eq!(
        status(connect(address, Some(&token)).await.unwrap_err()),
        429
    );
}

/// The phone answers every ping, so a socket quiet for two of them is a
/// path that stopped carrying traffic; one missed pong is not.
#[test]
fn a_socket_quiet_past_two_pings_is_closed() {
    assert!(QUIET_LIMIT >= PING_INTERVAL * 2);
    assert!(!too_quiet(PING_INTERVAL));
    assert!(!too_quiet(QUIET_LIMIT));
    assert!(too_quiet(QUIET_LIMIT + Duration::from_secs(1)));
}

/// A phone whose path went quiet stops taking bytes: a write that waits
/// past its limit ends the socket rather than holding it open.
#[tokio::test]
async fn a_write_that_cannot_go_out_ends_the_socket() {
    let limit = Duration::from_millis(20);
    let stuck = std::future::pending::<Result<(), ()>>();
    assert!(matches!(bounded(limit, stuck).await, Some(Ending::Stalled)));
    assert!(matches!(
        bounded(limit, async { Err::<(), _>(()) }).await,
        Some(Ending::SendFailed)
    ));
    assert!(bounded(limit, async { Ok::<(), ()>(()) }).await.is_none());
    assert!(WRITE_LIMIT < QUIET_LIMIT);
}

/// A chat's snapshot can run to megabytes: its write gets the time a slow
/// but working connection needs, while a small one keeps the short limit.
#[test]
fn a_large_message_gets_time_to_go_out() {
    assert_eq!(write_limit(0), WRITE_LIMIT);
    assert_eq!(write_limit(1024), WRITE_LIMIT);
    let five_megabytes = 5 * 1024 * 1024;
    assert_eq!(
        write_limit(five_megabytes),
        WRITE_LIMIT + Duration::from_secs(five_megabytes as u64 / SLOWEST_BYTES_PER_SEC)
    );
    assert!(write_limit(five_megabytes) >= Duration::from_secs(5 * 60));
    assert!(write_limit(usize::MAX) > WRITE_LIMIT);
}

/// No ping goes out while a long write does, so its time is not silence:
/// a snapshot that took minutes must not close the socket at the next ping.
#[test]
fn time_spent_writing_is_not_quiet() {
    let heard = Instant::now();
    let now = heard + Duration::from_secs(300);
    let after = not_counting(heard, Duration::from_secs(290), now);
    assert!(!too_quiet(now - after));
    assert_eq!(not_counting(heard, Duration::from_secs(400), now), now);
    assert_eq!(not_counting(heard, Duration::ZERO, now), heard);
}
