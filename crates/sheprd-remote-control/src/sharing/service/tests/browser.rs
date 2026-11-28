//! Synthetic host driven by scripts/test-share-viewer.py using a private tempdir.
use super::*;
use std::{fs, net::SocketAddr, path::PathBuf};
use tokio::net::TcpListener;

async fn signal(dir: &Path, name: &str) {
    tokio::time::timeout(Duration::from_secs(50), async {
        while !dir.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "run scripts/test-share-viewer.py with a headless browser"]
async fn browser_share_fixture() {
    let dir = PathBuf::from(std::env::var_os("SHEPRD_VIEWER_FIXTURE").unwrap());
    let port: u16 = std::env::var("SHEPRD_VIEWER_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let host = format!("127.0.0.1:{port}/relay");
    let relay = Endpoint::parse(&format!("https://{host}")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (endpoint, info) =
        sheprd_relay::bind_quic("127.0.0.1:0".parse().unwrap(), None, None).unwrap();
    let quic = endpoint.local_addr().unwrap();
    let pin = URL_SAFE_NO_PAD
        .decode(&info.pin)
        .unwrap()
        .try_into()
        .unwrap();
    let options = sheprd_relay::Options {
        public_host: Some(host),
        share_viewer: true,
        ..Default::default()
    };
    let (router, hub) = sheprd_relay::app(&options, Some(info)).unwrap();
    let router = axum::Router::new().nest("/relay", router);
    tokio::spawn(sheprd_relay::quic::run(endpoint, hub));
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let store = dir.join("store");
    let service = tokio::task::spawn_blocking(move || Service::open(&store))
        .await
        .unwrap()
        .unwrap();
    fs::write(dir.join("http"), addr.port().to_string()).unwrap();
    for scenario in ["revocation", "ended", "expiry"] {
        let endpoint = EndpointId::random();
        let mut cursor = Cursor::new_epoch();
        let mut transcript = sample_transcript();
        let snapshot = |cursor, transcript| {
            Some(Snapshot {
                endpoint: endpoint.clone(),
                cursor,
                transcript: Arc::new(transcript),
            })
        };
        let (tx, feed) = watch::channel(snapshot(cursor, transcript.clone()));
        let created = service
            .create_on(
                "synthetic",
                feed,
                tokio::spawn(std::future::pending()),
                &relay,
                client::connect(quic, pin).await.unwrap(),
                &[],
                ShareHistory::All,
                true,
                Duration::from_secs(if scenario == "expiry" { 6 } else { 120 }),
            )
            .await
            .unwrap();
        let tmp = dir.join("new-link");
        fs::write(&tmp, created.link.expose()).unwrap();
        fs::rename(tmp, dir.join(scenario)).unwrap();
        if scenario == "revocation" {
            signal(&dir, "update").await;
            transcript.entries.push(Entry { id:EntryId(5),kind:EntryKind::Agent {
                text:"**Regression test passes.**\n![image](https://example.invalid/image)\n<img src=x>".into()
            }});
            cursor.seq += 1;
            tx.send(snapshot(cursor, transcript.clone())).unwrap();
        }
        if scenario == "ended" {
            signal(&dir, "end-session").await;
            transcript.status = sheprd_chat_protocol::Status::Exited {
                message: "private".into(),
            };
            cursor.seq += 1;
            tx.send(snapshot(cursor, transcript)).unwrap();
        }
        signal(&dir, &format!("finish-{scenario}")).await;
        if scenario != "expiry" {
            service
                .revoke(ShareId(created.share.share), None)
                .await
                .unwrap();
        }
    }
}
