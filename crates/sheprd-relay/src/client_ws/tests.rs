//! The WebSocket client against the real relay, run in-process by
//! [`crate::testing`].

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod dialer;
mod framed;
mod listener;

use super::*;
use crate::testing::TestRelay;
use p256::ecdsa::SigningKey;
use tokio::sync::watch;

pub(crate) fn key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).unwrap()
}

/// Quick retries, for tests that restart the relay.
pub(crate) fn listener_config(relay: &TestRelay, key: SigningKey) -> ListenerConfig {
    ListenerConfig {
        backoff: Backoff {
            first: Duration::from_millis(50),
            max: Duration::from_millis(200),
        },
        ..ListenerConfig::new(relay.endpoint().clone(), key)
    }
}

/// Waits up to ten seconds for the listener's status to satisfy `wanted`.
pub(crate) async fn until(
    status: &mut watch::Receiver<ListenerStatus>,
    wanted: impl Fn(&ListenerStatus) -> bool,
) -> ListenerStatus {
    tokio::time::timeout(Duration::from_secs(10), status.wait_for(|now| wanted(now)))
        .await
        .expect("the listener did not reach the status in time")
        .expect("the listener stopped")
        .clone()
}

/// A listener on `relay` that allows `dialers`, once it is listening.
pub(crate) async fn listening(
    relay: &TestRelay,
    key: SigningKey,
    dialers: &[&SigningKey],
) -> Listener {
    let listener = Listener::spawn(listener_config(relay, key));
    let mut status = listener.status();
    until(&mut status, |now| *now == ListenerStatus::Listening).await;
    let keys = dialers.iter().map(|dialer| public(dialer)).collect();
    assert_eq!(
        listener.set_dial_keys(keys).await.unwrap(),
        Applied::Answered(Status::Ok)
    );
    listener
}

/// The next accepted dial, within ten seconds.
pub(crate) async fn accepted(listener: &mut Listener) -> Incoming {
    tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .expect("no dial in time")
        .expect("the listener stopped")
        .expect("the accept failed")
}
