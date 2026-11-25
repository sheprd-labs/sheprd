//! `POST /api/apns` tells the phone how this computer delivers its pushes,
//! so its Settings can show it.

use super::*;
use axum::http::{Request as HttpRequest, StatusCode, header};

fn register(token: &str) -> HttpRequest<Body> {
    HttpRequest::post("/api/apns")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(json!({ "activities": [] }).to_string()))
        .unwrap()
}

async fn delivery(apns: crate::apns::Delivery) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    Arc::get_mut(&mut state).unwrap().apns = apns;
    let (_, token) = signed_in(&state, 1);
    let (status, _, text) = send(&state, register(&token)).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str::<serde_json::Value>(&text).unwrap()["delivery"].clone()
}

#[tokio::test]
async fn the_answer_names_the_push_relay() {
    assert_eq!(
        delivery(crate::apns::Delivery::Relay(Box::new(
            crate::apns::Relays::new().unwrap()
        )))
        .await,
        "push_relay"
    );
}

#[tokio::test]
async fn grouping_is_registered_per_device_and_older_phones_keep_separate_cards() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    Arc::get_mut(&mut state).unwrap().apns = crate::apns::Delivery::new(true).unwrap();
    let (first, first_token) = signed_in(&state, 1);
    let (second, second_token) = signed_in(&state, 2);
    let epoch = uuid::Uuid::new_v4();
    let reset = format!("/api/apns?single=true&epoch={epoch}");
    for (token, path) in [(&first_token, reset.as_str()), (&second_token, "/api/apns")] {
        let req = HttpRequest::post(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(
                json!({"push_to_start": "ab".repeat(32)}).to_string(),
            ))
            .unwrap();
        assert_eq!(send(&state, req).await.0, StatusCode::OK);
    }
    let mode = |device| {
        crate::apns::Registration::from_stored(
            state
                .auth()
                .devices()
                .get(device)
                .and_then(|r| r.apns.as_ref()),
        )
    };
    assert!(mode(&first).single_activity);
    assert_eq!(mode(&first).activity_epoch, Some(epoch));
    assert!(!mode(&second).single_activity);
    assert_eq!(mode(&second).activity_epoch, None);
    assert_eq!(send(&state, register(&first_token)).await.0, StatusCode::OK);
    assert!(!mode(&first).single_activity);
    assert_eq!(mode(&first).activity_epoch, None);
}
