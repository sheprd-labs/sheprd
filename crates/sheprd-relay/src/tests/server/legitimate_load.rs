//! Capacity pressure from valid clients must not turn into an IP ban.

use super::*;
use crate::metrics::{Kind, Refused};
use axum::{
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn connection_capacity_refuses_excess_work_without_blocking_discovery() {
    let options = Options {
        behind_proxy: true,
        ..options(Limits {
            max_connections_per_ip: 2,
            ..Limits::default()
        })
    };
    let (router, hub) = app(&options, None).unwrap();
    let address = "203.0.113.80".parse().unwrap();
    let first = hub.admit(address, Kind::Quic).unwrap();
    let second = hub.admit(address, Kind::Quic).unwrap();
    for _ in 0..100 {
        assert!(matches!(
            hub.admit(address, Kind::Quic),
            Err(Refused::PerAddress)
        ));
    }
    assert!(!hub.is_banned(address));
    let router = router.layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 1], 1))));
    let response = router
        .oneshot(
            Request::get("/v1/info")
                .header("x-forwarded-for", "203.0.113.80")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    drop(first);
    assert!(hub.admit(address, Kind::Quic).is_ok());
    drop(second);
}

#[tokio::test]
async fn valid_dial_retries_are_rate_limited_without_banning_the_shared_address() {
    let hub = Arc::new(Hub::new(Limits::default(), crate::hub::Access::Open));
    let route = RouteId::of(signing_key(180).verifying_key());
    let listener = hub
        .register(route, "198.51.100.80".parse().unwrap())
        .unwrap();
    let key = signing_key(181);
    listener.set_dial_keys(&[public(&key)]);
    let address = "203.0.113.81".parse().unwrap();
    let held: Vec<_> = (0..hub.limits.max_sessions_per_key)
        .map(|_| hub.dial(route, public(&key), address).unwrap().unwrap())
        .collect();
    let signed = b"valid dial proof";
    let signature = sign(&key, signed);
    let mut limited = false;
    for _ in 0..100 {
        let result =
            crate::broker::dial(&hub, address, route, &public(&key), &signature, signed).await;
        match result {
            Err(Status::Busy) => {}
            Err(Status::Limited) => limited = true,
            _ => panic!("expected capacity or rate refusal"),
        }
    }
    assert!(limited);
    assert!(!hub.is_banned(address));
    assert!(!hub.is_banned("198.51.100.80".parse().unwrap()));
    drop(held);
    assert!(hub.dial(route, public(&key), address).unwrap().is_ok());
}
