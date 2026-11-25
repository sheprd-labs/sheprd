//! Many people behind one address, as in a household or a carrier's NAT:
//! running into limits never bans the address, and each side pays only for
//! what it sends, slowed rather than banned over its budget.

use super::*;
use crate::{
    hub::{Access, Session},
    metrics::{Gauge, Offense},
};
use axum::{
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode},
};
use std::{net::IpAddr, sync::atomic::Ordering::Relaxed};
use tower::ServiceExt;

const GIB: u64 = 1024 * 1024 * 1024;

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn hub() -> Arc<Hub> {
    Arc::new(Hub::new(Limits::default(), Access::Open))
}

/// A session from `dialer` to a computer at `listener`, on `hub`.
fn session(hub: &Arc<Hub>, listener: IpAddr, dialer: IpAddr, seed: u8) -> Session {
    let route = RouteId::of(signing_key(seed).verifying_key());
    let registration = hub.register(route, listener).unwrap();
    registration.set_dial_keys(&[public(&phone())]);
    let waiting = hub.dial(route, public(&phone()), dialer).unwrap().unwrap();
    let session = hub.open(waiting.slot).unwrap();
    std::mem::forget(registration);
    session
}

/// Repeated pressure from several clients sharing an address.
fn hit_limits(hub: &Arc<Hub>, address: IpAddr) {
    for _ in 0..100 {
        hub.offend(address, Offense::Limited);
        hub.offend(address, Offense::ConnectionCap);
    }
}

async fn ended(session: &Session) -> bool {
    tokio::time::timeout(Duration::from_millis(200), session.banned())
        .await
        .is_ok()
}

#[tokio::test]
async fn limits_alone_keep_sessions_listeners_and_new_work_available() {
    let hub = hub();
    let household = ip("203.0.113.40");
    let computer = session(&hub, household, ip("198.51.100.50"), 140);
    let on_phone = session(&hub, ip("198.51.100.51"), household, 141);
    hit_limits(&hub, household);
    assert!(!hub.is_banned(household));
    assert!(!hub.is_evicted(household));
    assert!(!ended(&computer).await);
    assert!(!ended(&on_phone).await);
    assert_eq!(hub.metrics.read(Gauge::Listeners), 2);
    // Its listener still takes dials; another computer may listen too.
    let route = RouteId::of(signing_key(140).verifying_key());
    let waiting = hub
        .dial(route, public(&phone()), ip("198.51.100.52"))
        .unwrap()
        .unwrap();
    assert!(hub.open(waiting.slot).is_some());
    let fresh = RouteId::of(signing_key(142).verifying_key());
    assert!(hub.register(fresh, household).is_ok());
}

#[tokio::test]
async fn a_ban_any_severe_offense_led_to_ends_everything() {
    let hub = hub();
    let toxic = ip("203.0.113.41");
    let computer = session(&hub, toxic, ip("198.51.100.53"), 143);
    // Capacity pressure is harmless, but repeated malformed messages are not.
    hit_limits(&hub, toxic);
    for _ in 0..crate::guard::BAN_POINTS {
        hub.offend(toxic, Offense::Malformed);
    }
    assert!(hub.is_evicted(toxic));
    assert!(ended(&computer).await);
    assert_eq!(hub.metrics.read(Gauge::Listeners), 0);
}

#[tokio::test]
async fn limits_do_not_block_http_discovery_or_websocket_accepts() {
    let options = Options {
        behind_proxy: true,
        ..options(Limits::default())
    };
    let (router, hub) = app(&options, None).unwrap();
    let router = router.layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 1], 1))));
    let session = crate::SessionId::random().unwrap();
    let send = |path: String, from: &'static str| {
        let router = router.clone();
        async move {
            let request = Request::get(path)
                .header("x-forwarded-for", from)
                .body(Body::empty())
                .unwrap();
            router.oneshot(request).await.unwrap().status()
        }
    };
    let household = ip("203.0.113.42");
    hit_limits(&hub, household);
    assert_eq!(
        send("/v1/info".to_owned(), "203.0.113.42").await,
        StatusCode::OK
    );
    // Not refused for the ban: past it, the request is no WebSocket.
    let accept = send(format!("/v1/accept/{session}"), "203.0.113.42").await;
    assert_ne!(accept, StatusCode::FORBIDDEN);
    // A ban that evicts refuses the accept too.
    let toxic = ip("203.0.113.43");
    while !hub.is_banned(toxic) {
        hub.offend(toxic, Offense::Malformed);
    }
    assert_eq!(
        send(format!("/v1/accept/{session}"), "203.0.113.43").await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn an_hour_at_full_speed_both_ways_bans_neither_side() {
    let hub = hub();
    let computer = ip("203.0.113.44");
    let phone = ip("198.51.100.54");
    let session = session(&hub, computer, phone, 144);
    // About an hour at the 4 MiB/s session limit, each way.
    let hour = Limits::default().session_bandwidth * 3600;
    session.traffic.from_dialer.fetch_add(hour, Relaxed);
    session.traffic.from_listener.fetch_add(hour, Relaxed);
    crate::splice::settle(&session, &crate::splice::Ledger::default());
    assert!(!hub.is_banned(computer));
    assert!(!hub.is_banned(phone));
    assert!(!hub.over_budget(computer));
    assert!(!hub.over_budget(phone));
    assert!(!ended(&session).await);
}

#[tokio::test]
async fn a_side_over_budget_is_slowed_and_its_peer_is_not_charged() {
    let hub = hub();
    let computer = ip("203.0.113.45");
    let phone = ip("198.51.100.55");
    let session = session(&hub, computer, phone, 145);
    let budget = Limits::default().max_transfer_per_hour;
    session
        .traffic
        .from_listener
        .fetch_add(budget + GIB, Relaxed);
    crate::splice::settle(&session, &crate::splice::Ledger::default());
    assert!(hub.over_budget(computer));
    assert!(!hub.is_banned(computer));
    assert!(!hub.over_budget(phone));
    assert!(!ended(&session).await);
}

#[tokio::test]
async fn work_kept_through_capacity_pressure_is_still_counted() {
    let hub = hub();
    let household = ip("203.0.113.46");
    let computer = session(&hub, household, ip("198.51.100.56"), 146);
    hit_limits(&hub, household);
    // Its transfer still counts toward the budget ...
    let budget = Limits::default().max_transfer_per_hour;
    hub.charge(household, budget + 1);
    assert!(hub.over_budget(household));
    assert!(!hub.is_evicted(household));
    assert!(!ended(&computer).await);
    // ... and repeated malformed messages still end its work.
    for _ in 0..crate::guard::BAN_POINTS {
        hub.offend(household, Offense::Malformed);
    }
    assert!(hub.is_evicted(household));
    assert!(ended(&computer).await);
}

#[tokio::test]
async fn overrunning_the_budget_after_capacity_pressure_evicts() {
    let hub = hub();
    let household = ip("203.0.113.47");
    let computer = session(&hub, household, ip("198.51.100.57"), 147);
    hit_limits(&hub, household);
    let overrun = Limits::default().max_transfer_per_hour * crate::guard::OVERRUN;
    hub.charge(household, overrun + 1);
    assert!(hub.is_evicted(household));
    assert!(ended(&computer).await);
}
