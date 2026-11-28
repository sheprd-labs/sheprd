//! A listener's message ids come back in the relay's status, as given;
//! a message without one gets a status without one.

use super::*;

#[tokio::test]
async fn statuses_echo_the_id_they_answer() {
    let (address, _) = start(&options(Limits::default())).await;
    let (mut listener, _) = listen_only(address, &signing_key(90), None).await;
    let phone = URL_SAFE_NO_PAD.encode(public(&phone()));
    for (sent, expected) in [
        (
            format!(r#"{{"t":"dial_keys","id":7,"keys":["{phone}"]}}"#),
            r#"{"t":"status","id":7,"status":"ok"}"#,
        ),
        (
            format!(r#"{{"t":"pairing_key","id":"pair","key":"{phone}","seconds":30}}"#),
            r#"{"t":"status","id":"pair","status":"ok"}"#,
        ),
        (
            format!(r#"{{"t":"dial_keys","keys":["{phone}"]}}"#),
            r#"{"t":"status","status":"ok"}"#,
        ),
    ] {
        listener.send(Frame::Text(sent.into())).await.unwrap();
        match next(&mut listener).await {
            Some(Frame::Text(text)) => assert_eq!(text.as_str(), expected),
            other => panic!("expected a status, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn an_id_too_long_breaks_the_protocol() {
    let (address, _) = start(&options(Limits::default())).await;
    let (mut listener, _) = listen_only(address, &signing_key(91), None).await;
    let id = "x".repeat(socket::MAX_ID + 1);
    let sent = format!(r#"{{"t":"dial_keys","id":"{id}","keys":[]}}"#);
    listener.send(Frame::Text(sent.into())).await.unwrap();
    assert_eq!(
        close_code(&mut listener).await,
        Some(socket::code::PROTOCOL)
    );
}
