//! WebSocket pings inside a session count as activity: a session that
//! carries only pings outlives the idle timeout, and one that carries
//! nothing does not. Pings are answered, never forwarded, never charged.

use super::*;
use crate::splice::code;

fn quick_idle() -> Options {
    options(Limits {
        idle_timeout: Duration::from_millis(1_200),
        ..Limits::default()
    })
}

#[tokio::test]
async fn a_session_with_only_pings_outlives_the_idle_timeout() {
    let (address, hub) = start(&quick_idle()).await;
    let (mut listener, route) = listen(address, &signing_key(70), None).await;
    let (mut dialer, mut accepted) = session(address, &mut listener, &route).await;
    // Pings from each side in turn, for more than twice the timeout.
    for turn in 0..12 {
        let socket = if turn % 2 == 0 {
            &mut dialer
        } else {
            &mut accepted
        };
        socket.send(Frame::Ping(vec![1, 2].into())).await.unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    // The session is still open both ways, and the pings never crossed it.
    dialer.send(Frame::Binary(vec![7].into())).await.unwrap();
    assert_eq!(
        next(&mut accepted).await,
        Some(Frame::Binary(vec![7].into()))
    );
    accepted.send(Frame::Binary(vec![8].into())).await.unwrap();
    assert_eq!(next(&mut dialer).await, Some(Frame::Binary(vec![8].into())));
    let totals = hub.metrics.totals();
    assert_eq!(totals.bytes_from_dialers + totals.bytes_from_listeners, 2);
}

#[tokio::test]
async fn the_relay_answers_a_session_ping_itself() {
    let (address, _) = start(&quick_idle()).await;
    let (mut listener, route) = listen(address, &signing_key(71), None).await;
    let (mut dialer, mut accepted) = session(address, &mut listener, &route).await;
    dialer.send(Frame::Ping(vec![9].into())).await.unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Ok(Frame::Pong(payload))) = dialer.next().await {
                return payload;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(pong.as_ref(), [9]);
    // Nothing reached the listener's side but the eventual close.
    assert_eq!(close_code(&mut accepted).await, Some(code::IDLE));
}

#[tokio::test]
async fn a_session_with_nothing_crossing_idles_out() {
    let (address, _) = start(&quick_idle()).await;
    let (mut listener, route) = listen(address, &signing_key(72), None).await;
    let (mut dialer, mut accepted) = session(address, &mut listener, &route).await;
    let started = tokio::time::Instant::now();
    assert_eq!(close_code(&mut dialer).await, Some(code::IDLE));
    assert_eq!(close_code(&mut accepted).await, Some(code::IDLE));
    assert!(started.elapsed() >= Duration::from_millis(1_000));
}
