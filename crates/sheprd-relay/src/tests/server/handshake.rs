//! Real WebSockets through the proxy path: interruptions must not poison a
//! shared cellular address, while invalid application data still earns bans.
use crate::metrics::{Gauge, Kind};
use crate::tests::server::*;
use std::net::IpAddr;

const CALLER: &str = "203.0.113.80";

async fn connect_caller(address: SocketAddr, path: &str) -> Socket {
    let mut request = format!("ws://{address}{path}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("x-forwarded-for", CALLER.parse().unwrap());
    let stream = TcpStream::connect(address).await.unwrap();
    client_async(request, stream).await.unwrap().0
}

async fn relay() -> (SocketAddr, Arc<Hub>) {
    start(&Options {
        behind_proxy: true,
        ..options(Limits::default())
    })
    .await
}

fn paths() -> [String; 2] {
    [
        "/v1/listen".into(),
        format!("/v1/dial/{}", RouteId::of(signing_key(51).verifying_key())),
    ]
}

async fn drained(hub: &Hub) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while [Kind::Listen, Kind::Dial]
            .iter()
            .any(|kind| hub.metrics.read(Gauge::Connections(*kind)) != 0)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("handshake retained its connection permit");
}

fn no_strikes(hub: &Hub) {
    assert!(!hub.is_banned(CALLER.parse::<IpAddr>().unwrap()));
    let metrics = hub.metrics.render();
    assert!(
        metrics.contains("sheprd_relay_strikes_total{offense=\"malformed\"} 0"),
        "{metrics}"
    );
}

#[tokio::test]
async fn cancelled_handshakes_do_not_ban_a_shared_address() {
    for path in paths() {
        let (address, hub) = relay().await;
        for graceful in [true, false] {
            for _ in 0..5 {
                let mut peer = connect_caller(address, &path).await;
                challenge(&mut peer).await;
                if graceful {
                    peer.close(None).await.unwrap();
                }
                drop(peer);
                drained(&hub).await;
            }
        }
        no_strikes(&hub);
    }
}

#[tokio::test]
async fn handshake_timeout_is_not_malformed() {
    for path in paths() {
        let (address, hub) = relay().await;
        let mut peer = connect_caller(address, &path).await;
        challenge(&mut peer).await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(11)).await;
        tokio::time::resume();
        assert!(close_code(&mut peer).await.is_some());
        drained(&hub).await;
        no_strikes(&hub);
    }
}

#[tokio::test]
async fn control_frames_before_hello_do_not_break_authentication() {
    let (address, hub) = relay().await;
    let mut peer = connect_caller(address, "/v1/listen").await;
    let nonce = challenge(&mut peer).await;
    peer.send(Frame::Ping(vec![1].into())).await.unwrap();
    peer.send(Frame::Pong(vec![2].into())).await.unwrap();
    let proof = hello(
        &signing_key(52),
        &socket::listen_signed(&address.to_string(), &nonce),
    );
    send_json(&mut peer, &proof).await;
    assert!(matches!(
        next_control(&mut peer).await,
        ToPeer::Ready { .. }
    ));
    no_strikes(&hub);
}

#[tokio::test]
async fn malformed_handshakes_still_ban_the_caller() {
    for path in paths() {
        for frame in [
            Frame::Text("not json".into()),
            Frame::Binary(vec![1].into()),
        ] {
            let (address, hub) = relay().await;
            for _ in 0..4 {
                let mut peer = connect_caller(address, &path).await;
                challenge(&mut peer).await;
                peer.send(frame.clone()).await.unwrap();
                assert!(close_code(&mut peer).await.is_some());
                drained(&hub).await;
            }
            assert!(hub.is_banned(CALLER.parse().unwrap()));
        }
    }
}
