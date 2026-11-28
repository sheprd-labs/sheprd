//! A client written against the protocol before ids, neutral names, or
//! pings as activity: the exact JSON text and bytes it sends, and the
//! exact text it expects back. It relies on nothing added since, so it
//! must keep passing unchanged.

use super::*;
use p256::ecdsa::{Signature, signature::Signer};
use sha2::{Digest, Sha256};

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn sec1(key: &SigningKey) -> Vec<u8> {
    key.verifying_key().to_sec1_point(true).as_bytes().to_vec()
}

/// Signs `context ‖ name ‖ 0x00 ‖ nonce ‖ extra`, built by hand.
fn proof(key: &SigningKey, context: &[u8], name: &str, nonce: &[u8], extra: &[u8]) -> String {
    let message = [context, name.as_bytes(), &[0], nonce, extra].concat();
    let signature: Signature = key.sign(&message);
    format!(
        r#"{{"key":"{}","signature":"{}"}}"#,
        b64(&sec1(key)),
        b64(&signature.to_bytes())
    )
}

async fn text(socket: &mut Socket) -> String {
    match next(socket).await {
        Some(Frame::Text(text)) => text.to_string(),
        other => panic!("expected text, got {other:?}"),
    }
}

async fn send_text(socket: &mut Socket, text: &str) {
    socket
        .send(Frame::Text(text.to_owned().into()))
        .await
        .unwrap();
}

/// The nonce from a challenge, checking its exact shape.
async fn nonce_of(socket: &mut Socket) -> Vec<u8> {
    let challenge = text(socket).await;
    let nonce = challenge
        .strip_prefix(r#"{"t":"challenge","nonce":""#)
        .and_then(|rest| rest.strip_suffix(r#"","version":1}"#))
        .unwrap_or_else(|| panic!("challenge: {challenge}"));
    URL_SAFE_NO_PAD.decode(nonce).unwrap()
}

#[tokio::test]
async fn the_original_protocol_still_works_byte_for_byte() {
    let (address, _) = start(&options(Limits::default())).await;
    let name = address.to_string();
    let computer = signing_key(80);
    let phone = signing_key(81);
    let pairing = signing_key(82);
    let route = b64(&Sha256::digest(sec1(&computer)));

    let mut listener = connect(address, "/v1/listen").await.unwrap();
    let nonce = nonce_of(&mut listener).await;
    let hello = proof(&computer, b"sheprd-relay/listen/v1\0", &name, &nonce, &[]);
    send_text(&mut listener, &hello).await;
    assert_eq!(
        text(&mut listener).await,
        format!(r#"{{"t":"ready","route":"{route}"}}"#)
    );
    let ok = r#"{"t":"status","status":"ok"}"#;
    send_text(
        &mut listener,
        &format!(r#"{{"t":"dial_keys","keys":["{}"]}}"#, b64(&sec1(&phone))),
    )
    .await;
    assert_eq!(text(&mut listener).await, ok);
    let pairing_key = format!(
        r#"{{"t":"pairing_key","key":"{}","seconds":600}}"#,
        b64(&sec1(&pairing))
    );
    send_text(&mut listener, &pairing_key).await;
    assert_eq!(text(&mut listener).await, ok);

    for dialer in [&phone, &pairing] {
        let route_bytes = URL_SAFE_NO_PAD.decode(&route).unwrap();
        let dial = tokio::spawn({
            let dialer = dialer.clone();
            let name = name.clone();
            let route = route.clone();
            async move {
                let mut socket = connect(address, &format!("/v1/dial/{route}"))
                    .await
                    .unwrap();
                let nonce = nonce_of(&mut socket).await;
                let hello = proof(
                    &dialer,
                    b"sheprd-relay/dial/v1\0",
                    &name,
                    &nonce,
                    &route_bytes,
                );
                send_text(&mut socket, &hello).await;
                assert_eq!(text(&mut socket).await, r#"{"t":"status","status":"ok"}"#);
                socket
            }
        });
        let incoming = text(&mut listener).await;
        let expected_head = format!(
            r#"{{"t":"incoming","key":"{}","session":""#,
            b64(&sec1(dialer))
        );
        let session = incoming
            .strip_prefix(&expected_head)
            .and_then(|rest| rest.strip_suffix(r#""}"#))
            .unwrap_or_else(|| panic!("incoming: {incoming}"));
        assert_eq!(session.len(), 22);
        let mut accepted = connect(address, &format!("/v1/accept/{session}"))
            .await
            .unwrap();
        let mut dialed = dial.await.unwrap();
        dialed
            .send(Frame::Binary(vec![0, 1, 2].into()))
            .await
            .unwrap();
        assert_eq!(
            next(&mut accepted).await,
            Some(Frame::Binary(vec![0, 1, 2].into()))
        );
        accepted.send(Frame::Binary(vec![3].into())).await.unwrap();
        assert_eq!(next(&mut dialed).await, Some(Frame::Binary(vec![3].into())));
    }

    // The pairing key was used up: a second dial with it hears offline,
    // then closes with 4404.
    let mut again = connect(address, &format!("/v1/dial/{route}"))
        .await
        .unwrap();
    let nonce_again = nonce_of(&mut again).await;
    let route_bytes = URL_SAFE_NO_PAD.decode(&route).unwrap();
    send_text(
        &mut again,
        &proof(
            &pairing,
            b"sheprd-relay/dial/v1\0",
            &name,
            &nonce_again,
            &route_bytes,
        ),
    )
    .await;
    assert_eq!(
        text(&mut again).await,
        r#"{"t":"status","status":"offline"}"#
    );
    assert_eq!(close_code(&mut again).await, Some(4404));

    // A second listener with the same key replaces the first with 4409.
    let mut second = connect(address, "/v1/listen").await.unwrap();
    let nonce = nonce_of(&mut second).await;
    send_text(
        &mut second,
        &proof(&computer, b"sheprd-relay/listen/v1\0", &name, &nonce, &[]),
    )
    .await;
    assert_eq!(
        text(&mut second).await,
        format!(r#"{{"t":"ready","route":"{route}"}}"#)
    );
    assert_eq!(close_code(&mut listener).await, Some(4409));
}

#[tokio::test]
async fn info_keeps_its_fields() {
    let (address, _) = start(&options(Limits::default())).await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = "GET /v1/info HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap();
    let info: serde_json::Value = serde_json::from_str(body).unwrap();
    for field in ["protocol", "version", "listen_token", "quic", "websocket"] {
        assert!(info.get(field).is_some(), "{field} missing from {body}");
    }
    assert_eq!(info["protocol"], 1);
    assert_eq!(info["websocket"], true);
}
