//! The listener: pairing keys work once, new dial keys revoke old ones,
//! it comes back after the relay restarts with its keys, and it stops
//! for good when replaced.

use super::*;
use crate::testing::{self, TestRelay};
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn a_pairing_key_lets_one_dial_through() {
    let relay = TestRelay::start().unwrap();
    let mut listener = listening(&relay, key(20), &[]).await;
    let pairing = key(21);
    assert_eq!(
        listener
            .add_pairing_key(public(&pairing), 60)
            .await
            .unwrap(),
        Applied::Answered(Status::Ok)
    );
    let route = listener.route();
    let first = tokio::spawn({
        let endpoint = relay.endpoint().clone();
        let pairing = pairing.clone();
        async move { dial(&endpoint, &route, &pairing).await }
    });
    let incoming = accepted(&mut listener).await;
    assert_eq!(incoming.key, public(&pairing));
    first.await.unwrap().unwrap();
    let second = dial(relay.endpoint(), &route, &pairing).await;
    assert!(
        matches!(second, Err(Error::Refused(Status::Offline))),
        "{second:?}"
    );
}

#[tokio::test]
async fn new_dial_keys_revoke_a_key_left_out() {
    let relay = TestRelay::start().unwrap();
    let (old, new) = (key(23), key(24));
    let mut listener = listening(&relay, key(22), &[&old]).await;
    let route = listener.route();
    let dial_old = tokio::spawn({
        let endpoint = relay.endpoint().clone();
        let old = old.clone();
        async move { dial(&endpoint, &route, &old).await }
    });
    let _accepted = accepted(&mut listener).await;
    let mut session = dial_old.await.unwrap().unwrap();
    assert_eq!(
        listener.set_dial_keys(vec![public(&new)]).await.unwrap(),
        Applied::Answered(Status::Ok)
    );
    // The revoked key's open session ends, and it can no longer dial.
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), session.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    let again = dial(relay.endpoint(), &route, &old).await;
    assert!(
        matches!(again, Err(Error::Refused(Status::Offline))),
        "{again:?}"
    );
    let fresh = tokio::spawn({
        let endpoint = relay.endpoint().clone();
        async move { dial(&endpoint, &route, &new).await }
    });
    let _accepted = accepted(&mut listener).await;
    fresh.await.unwrap().unwrap();
}

#[tokio::test]
async fn it_comes_back_after_the_relay_restarts_with_its_keys() {
    let relay = TestRelay::start().unwrap();
    let address = relay.address();
    let (phone, pairing) = (key(26), key(27));
    let mut listener = listening(&relay, key(25), &[&phone]).await;
    listener
        .add_pairing_key(public(&pairing), 120)
        .await
        .unwrap();
    let mut status = listener.status();
    relay.stop();
    until(&mut status, |now| {
        matches!(now, ListenerStatus::Unreachable(_))
    })
    .await;
    // Changes while it is away are kept for the next connection.
    let both = vec![public(&phone), public(&key(28))];
    assert_eq!(listener.set_dial_keys(both).await.unwrap(), Applied::Queued);
    let relay = TestRelay::start_with(testing::Config {
        address,
        ..testing::Config::default()
    })
    .unwrap();
    until(&mut status, |now| *now == ListenerStatus::Listening).await;
    let route = listener.route();
    for dialer in [phone, key(28), pairing] {
        let dial = tokio::spawn({
            let endpoint = relay.endpoint().clone();
            let dialer = dialer.clone();
            async move { dial(&endpoint, &route, &dialer).await }
        });
        let incoming = accepted(&mut listener).await;
        assert_eq!(incoming.key, public(&dialer));
        dial.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn a_second_listener_replaces_the_first_for_good() {
    let relay = TestRelay::start().unwrap();
    let first = listening(&relay, key(30), &[]).await;
    let mut first_status = first.status();
    let second = listening(&relay, key(30), &[]).await;
    assert_eq!(
        until(&mut first_status, |now| *now == ListenerStatus::Replaced).await,
        ListenerStatus::Replaced
    );
    // It stays replaced rather than taking the route back.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(*first_status.borrow(), ListenerStatus::Replaced);
    assert_eq!(*second.status().borrow(), ListenerStatus::Listening);
    assert!(matches!(
        first.set_dial_keys(Vec::new()).await,
        Err(Error::Stopped)
    ));
}

#[tokio::test]
async fn a_wrong_listen_token_is_unreachable_not_fatal() {
    let relay = TestRelay::start_with(testing::Config {
        listen_tokens: Some("a listen token of enough length".into()),
        ..testing::Config::default()
    })
    .unwrap();
    let listener = Listener::spawn(ListenerConfig {
        token: Some("the wrong token, also long".into()),
        ..listener_config(&relay, key(31))
    });
    let mut status = listener.status();
    let reached = until(&mut status, |now| {
        matches!(now, ListenerStatus::Unreachable(_))
    })
    .await;
    assert_eq!(
        reached,
        ListenerStatus::Unreachable(Unreachable::Closed(Some(crate::socket::code::REFUSED)))
    );
    let right = Listener::spawn(ListenerConfig {
        token: Some("a listen token of enough length".into()),
        ..listener_config(&relay, key(32))
    });
    let mut status = right.status();
    until(&mut status, |now| *now == ListenerStatus::Listening).await;
}
