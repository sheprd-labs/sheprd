//! Dialing: a session both ways, refusals as typed statuses, and pings
//! keeping a quiet session open.

use super::*;
use crate::{limits::Limits, testing};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn a_dial_reaches_the_listener_and_bytes_cross_both_ways() {
    let relay = TestRelay::start().unwrap();
    let phone = key(2);
    let mut listener = listening(&relay, key(1), &[&phone]).await;
    let dial = tokio::spawn({
        let endpoint = relay.endpoint().clone();
        let route = listener.route();
        async move { dial(&endpoint, &route, &phone).await }
    });
    let Incoming {
        key: dialer,
        mut session,
    } = accepted(&mut listener).await;
    assert_eq!(dialer, public(&key(2)));
    let mut dialed = dial.await.unwrap().unwrap();
    dialed.write_all(b"from the phone").await.unwrap();
    dialed.flush().await.unwrap();
    let mut read = [0; 14];
    session.read_exact(&mut read).await.unwrap();
    assert_eq!(&read, b"from the phone");
    session.write_all(b"back").await.unwrap();
    session.flush().await.unwrap();
    let mut read = [0; 4];
    dialed.read_exact(&mut read).await.unwrap();
    assert_eq!(&read, b"back");
    // Ending one side ends the other.
    dialed.shutdown().await.unwrap();
    let mut rest = Vec::new();
    session.read_to_end(&mut rest).await.unwrap();
    assert!(rest.is_empty());
}

#[tokio::test]
async fn refusals_are_typed_statuses() {
    let relay = TestRelay::start().unwrap();
    let listener = listening(&relay, key(3), &[&key(4)]).await;
    // A key the listener does not name hears offline, as an empty route does.
    let stranger = dial(relay.endpoint(), &listener.route(), &key(5)).await;
    assert!(
        matches!(stranger, Err(Error::Refused(Status::Offline))),
        "{stranger:?}"
    );
    let empty = RouteId::of(key(6).verifying_key());
    let nobody = dial(relay.endpoint(), &empty, &key(4)).await;
    assert!(
        matches!(nobody, Err(Error::Refused(Status::Offline))),
        "{nobody:?}"
    );
}

#[tokio::test]
async fn pings_keep_a_quiet_session_open() {
    let relay = TestRelay::start_with(testing::Config {
        limits: Limits {
            idle_timeout: Duration::from_millis(1_000),
            ..testing::limits()
        },
        ..testing::Config::default()
    })
    .unwrap();
    let phone = key(10);
    let mut listener = listening(&relay, key(9), &[&phone]).await;
    let config = Config {
        keepalive: Some(Duration::from_millis(200)),
        ..Config::default()
    };
    let dial = tokio::spawn({
        let endpoint = relay.endpoint().clone();
        let route = listener.route();
        let config = config.clone();
        async move { dial_with(&endpoint, &route, &phone, &config).await }
    });
    let Incoming { mut session, .. } = accepted(&mut listener).await;
    let mut dialed = dial.await.unwrap().unwrap();
    // The dialer reads, and so pings, for well over the idle timeout.
    let reader = tokio::spawn(async move {
        let mut byte = [0; 1];
        dialed.read_exact(&mut byte).await.map(|_| byte[0])
    });
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    session.write_all(&[42]).await.unwrap();
    session.flush().await.unwrap();
    assert_eq!(reader.await.unwrap().unwrap(), 42);
}
