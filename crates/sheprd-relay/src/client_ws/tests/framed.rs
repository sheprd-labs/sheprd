//! Whole messages through a session: larger than a frame, many at once,
//! and a read cancelled midway losing nothing.

use super::*;

async fn pair(relay: &TestRelay, seed: u8, config: Config) -> (Session, Session) {
    let phone = key(seed + 1);
    let mut listener = listening(relay, key(seed), &[&phone]).await;
    let dial = tokio::spawn({
        let endpoint = relay.endpoint().clone();
        let route = listener.route();
        async move { dial_with(&endpoint, &route, &phone, &config).await }
    });
    let Incoming { session, .. } = accepted(&mut listener).await;
    (dial.await.unwrap().unwrap(), session)
}

#[tokio::test]
async fn messages_cross_whole_whatever_the_frames() {
    let relay = TestRelay::start().unwrap();
    let small_frames = Config {
        max_frame: 1_000,
        ..Config::default()
    };
    let (dialed, accepted) = pair(&relay, 40, small_frames).await;
    let (mut dialed, mut accepted) = (dialed.framed(65_535), accepted.framed(65_535));
    let messages: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"short".to_vec(),
        vec![0xab; 40_000],
        vec![1; 65_535],
    ];
    for message in &messages {
        dialed.send(message).await.unwrap();
    }
    for message in &messages {
        assert_eq!(accepted.recv().await.unwrap().as_ref(), Some(message));
    }
}

#[tokio::test]
async fn a_cancelled_read_loses_nothing() {
    let relay = TestRelay::start().unwrap();
    let (dialed, accepted) = pair(&relay, 42, Config::default()).await;
    let mut accepted = accepted.framed(1024);
    let mut raw = dialed;
    // One message, written a byte at a time with pauses, while the reader
    // gives up and starts again many times.
    let writer = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let framed = crate::framing::encode(b"patient").unwrap();
        for byte in framed {
            raw.write_all(&[byte]).await.unwrap();
            raw.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        raw
    });
    let message = loop {
        match tokio::time::timeout(Duration::from_millis(7), accepted.recv()).await {
            Ok(message) => break message.unwrap(),
            Err(_) => continue,
        }
    };
    assert_eq!(message.as_deref(), Some(&b"patient"[..]));
    drop(writer.await.unwrap());
}

#[tokio::test]
async fn a_message_over_the_cap_is_refused() {
    let relay = TestRelay::start().unwrap();
    let (dialed, accepted) = pair(&relay, 44, Config::default()).await;
    let (mut dialed, mut accepted) = (dialed.framed(65_535), accepted.framed(16));
    dialed.send(&[0; 17]).await.unwrap();
    assert!(matches!(
        accepted.recv().await,
        Err(Error::Framing(crate::framing::Error::OverCap {
            cap: 16,
            length: 17
        }))
    ));
}
