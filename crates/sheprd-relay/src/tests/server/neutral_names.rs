//! The neutral ban header and metric names, served beside the old ones,
//! which stay as deprecated aliases.

use super::*;
use crate::endpoint::{BAN_HEADER, NEUTRAL_BAN_HEADER, is_ban};

#[tokio::test]
async fn a_ban_carries_both_headers() {
    let options = Options {
        bans: vec!["127.0.0.0/8".parse().unwrap()],
        ..options(Limits::default())
    };
    let (address, _) = start(&options).await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = "GET /v1/info HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();
    let response = response.to_ascii_lowercase();
    assert!(response.starts_with("http/1.1 403"));
    assert!(response.contains(&format!("{BAN_HEADER}: 1")));
    assert!(response.contains(&format!("{NEUTRAL_BAN_HEADER}: 1")));
}

#[test]
fn either_header_marks_a_ban() {
    assert!(is_ban(|name| (name == BAN_HEADER).then_some("1")));
    assert!(is_ban(|name| (name == NEUTRAL_BAN_HEADER).then_some("1")));
    assert!(!is_ban(|_| None));
    assert!(!is_ban(|_| Some("0")));
}

#[test]
fn every_family_is_served_under_both_names() {
    let hub = Hub::new(Limits::default(), crate::hub::Access::Open);
    let text = crate::metrics::with_neutral_names(&hub.metrics.render());
    for name in ["sessions", "dials_total", "session_duration_seconds"] {
        assert!(
            text.contains(&format!("# TYPE sheprd_relay_{name} ")),
            "{name}"
        );
        assert!(text.contains(&format!("# TYPE relay_{name} ")), "{name}");
    }
    let legacy = text
        .lines()
        .filter(|line| line.starts_with("sheprd_relay_"))
        .count();
    let neutral = text
        .lines()
        .filter(|line| line.starts_with("relay_"))
        .count();
    assert_eq!(legacy, neutral);
}
