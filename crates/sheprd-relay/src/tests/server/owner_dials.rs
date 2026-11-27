//! A private relay lets its owners' devices dial past the per-address rate,
//! while every other dial past it still answers `Limited`.

use super::*;
use crate::hub::Access;
use p256::ecdsa::SigningKey;
use std::{net::IpAddr, time::Duration};

const TOKEN: &str = "a token of sixteen chars";

/// A hub with one listener allowing `owner`, and `address` past its dial rate.
fn spent(
    access: Access,
    owner: &SigningKey,
) -> (Arc<Hub>, RouteId, IpAddr, crate::hub::Registration) {
    let limits = Limits {
        accept_timeout: Duration::from_millis(20),
        ..Limits::default()
    };
    let hub = Arc::new(Hub::new(limits, access));
    let route = RouteId::of(signing_key(190).verifying_key());
    let listener = hub
        .register(route, "198.51.100.90".parse().unwrap())
        .unwrap();
    listener.set_dial_keys(&[public(owner)]);
    let address: IpAddr = "203.0.113.90".parse().unwrap();
    while hub.allow_dial(address) == crate::bucket::Outcome::Allowed {}
    (hub, route, address, listener)
}

async fn dial(hub: &Arc<Hub>, route: RouteId, address: IpAddr, key: &SigningKey) -> Status {
    let signed = b"dial proof";
    let signature = sign(key, signed);
    match crate::broker::dial(hub, address, route, &public(key), &signature, signed).await {
        Ok(_) => Status::Ok,
        Err(status) => status,
    }
}

#[tokio::test]
async fn an_owner_dials_past_the_rate_on_a_private_relay() {
    let owner = signing_key(191);
    let (hub, route, address, _listener) = spent(Access::tokens(TOKEN).unwrap(), &owner);
    // Nothing accepts, so each times out: each got past the rate. Fewer
    // than the listener's queue of unread dials, which nothing reads here.
    for _ in 0..10 {
        assert_eq!(dial(&hub, route, address, &owner).await, Status::Timeout);
    }
}

#[tokio::test]
async fn a_stranger_past_the_rate_learns_nothing_on_a_private_relay() {
    let owner = signing_key(192);
    let (hub, route, address, _listener) = spent(Access::tokens(TOKEN).unwrap(), &owner);
    let stranger = signing_key(193);
    assert_eq!(dial(&hub, route, address, &stranger).await, Status::Limited);
    let elsewhere = RouteId::of(signing_key(194).verifying_key());
    assert_eq!(
        dial(&hub, elsewhere, address, &owner).await,
        Status::Limited
    );
    let signed = b"dial proof";
    let forged = sign(&stranger, signed);
    let result = crate::broker::dial(&hub, address, route, &public(&owner), &forged, signed).await;
    assert!(matches!(result, Err(Status::Limited)));
}

#[tokio::test]
async fn an_open_relay_keeps_its_dial_rate_for_everyone() {
    let owner = signing_key(195);
    let (hub, route, address, _listener) = spent(Access::Open, &owner);
    assert_eq!(dial(&hub, route, address, &owner).await, Status::Limited);
}

#[tokio::test]
async fn a_pairing_key_keeps_the_dial_rate_on_a_private_relay() {
    let owner = signing_key(196);
    let (hub, route, address, listener) = spent(Access::tokens(TOKEN).unwrap(), &owner);
    let invited = signing_key(197);
    assert!(listener.add_pairing_key(public(&invited), Duration::from_secs(60)));
    assert_eq!(dial(&hub, route, address, &invited).await, Status::Limited);
}

#[tokio::test]
async fn forged_proofs_past_the_rate_cost_the_same_named_or_not() {
    let owner = signing_key(198);
    let (hub, route, address, _listener) = spent(Access::tokens(TOKEN).unwrap(), &owner);
    let stranger = signing_key(199);
    let signed = b"dial proof";
    let forged = sign(&stranger, signed);
    for _ in 0..20 {
        let result =
            crate::broker::dial(&hub, address, route, &public(&owner), &forged, signed).await;
        assert!(matches!(result, Err(Status::Limited)));
    }
    assert!(!hub.is_banned(address));
}

#[tokio::test]
async fn forged_proofs_past_the_rate_stop_being_checked() {
    let owner = signing_key(200);
    let (hub, route, address, _listener) = spent(Access::tokens(TOKEN).unwrap(), &owner);
    let signed = b"dial proof";
    let forged = sign(&signing_key(201), signed);
    assert!(hub.may_prove_past_rate(address));
    for _ in 0..100 {
        let result =
            crate::broker::dial(&hub, address, route, &public(&owner), &forged, signed).await;
        assert!(matches!(result, Err(Status::Limited)));
    }
    assert!(!hub.may_prove_past_rate(address));
    assert!(!hub.is_banned(address));
}

#[tokio::test]
async fn refusals_past_the_rate_spend_alike_named_or_not() {
    let owner = signing_key(202);
    let (hub, route, address, _listener) = spent(Access::tokens(TOKEN).unwrap(), &owner);
    let stranger = signing_key(203);
    for _ in 0..100 {
        assert_eq!(dial(&hub, route, address, &stranger).await, Status::Limited);
    }
    // An unnamed key emptied the budget as a forged named one would.
    assert!(!hub.may_prove_past_rate(address));
    assert_eq!(dial(&hub, route, address, &owner).await, Status::Limited);
}
