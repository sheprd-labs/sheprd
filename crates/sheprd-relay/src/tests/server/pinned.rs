//! A client given the pin Sheprd's signed list carries reaches a relay
//! whose certificate has it, and refuses one whose `/v1/info` names
//! another, before dialing.

use super::*;
use crate::client::{self, reach_pinned};

#[tokio::test]
async fn the_expected_pin_is_reached_and_another_refused() {
    let relay = start_both(&options(Limits::default())).await;
    let endpoint = format!("http://{}", relay.http).parse().unwrap();
    assert!(reach_pinned(&endpoint, Some(relay.pin)).await.is_ok());
    let mut other = relay.pin;
    other[0] ^= 1;
    assert!(matches!(
        reach_pinned(&endpoint, Some(other)).await,
        Err(client::Error::PinMismatch)
    ));
}
