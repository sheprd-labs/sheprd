use super::*;
use axum::{body::Body, extract::connect_info::MockConnectInfo, http::Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn crawler_only_receives_a_static_landing_page_on_a_websocket_relay() {
    let quic = crate::endpoint::QuicInfo {
        pin: crate::endpoint::encode_pin(&[0; 32]),
        port: 8443,
    };
    for (enabled, public, udp) in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
        (true, true, true),
    ] {
        let capable = enabled && public && udp;
        let options = Options {
            share_viewer: enabled,
            public_host: public.then(|| "relay.example".into()),
            ..Options::default()
        };
        let (app, hub) = app(&options, udp.then(|| quic.clone())).unwrap();
        let app = app.layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 443))));
        let response = app
            .clone()
            .oneshot(Request::get("/v1/info").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let info: crate::endpoint::Info = serde_json::from_slice(&body).unwrap();
        assert_eq!(info.share_viewer, capable);
        for path in [
            "",
            "viewer.js",
            "worker.js",
            "guest.js",
            "guest_bg.wasm",
            "viewer.css",
        ] {
            let request = Request::get(format!("/share/{path}"))
                .header("user-agent", "LinkPreview")
                .body(Body::empty())
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            if !capable {
                assert_eq!(response.status(), 404);
                continue;
            }
            assert_eq!(response.status(), 200);
            let headers = response.headers();
            assert_eq!(headers["cache-control"], "no-store");
            assert_eq!(headers["referrer-policy"], "no-referrer");
            assert_eq!(headers["x-frame-options"], "DENY");
            let policy = headers["content-security-policy"].to_str().unwrap();
            assert!(policy.contains("frame-ancestors 'none'"));
            assert!(!policy.contains("'unsafe-inline'") && !policy.contains("'unsafe-eval'"));
            if path.is_empty() {
                let body = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(&body[..], include_bytes!("../../viewer/assets/index.html"));
            }
        }
        assert_eq!(hub.metrics.read(crate::metrics::Gauge::Listeners), 0);
    }
}
