//! `POST /api/apns/test`: the phone asks this computer to send it one
//! sealed alert now, through the push relay, to see notifications arrive.
//! Sheprd asks the same for every phone through `Request::TestPush`.
//! The answer is what the relay said; the phone's extension records the id when the alert opens, so the
//! app can tell whether, and how fast, it came.

use super::{ApiError, Device, Shared, now_ms};
use crate::apns::{
    self, Registration, Sent,
    alert::{Sealed, valid_test_id},
};
use crate::control::DeviceId;
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TestRequest {
    id: String,
}

/// What the push relay answered. It gives a status but no reason, so an
/// app that reads `reason` finds none.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub(super) enum TestAnswer {
    Delivered,
    /// APNs no longer knows this phone's alert handle: it is forgotten, and
    /// the phone registers a new one when it next opens.
    Gone,
    Limited,
    Refused {
        status: u16,
    },
    /// The relay could not be reached.
    Unreachable,
}

pub(super) async fn send(
    State(state): State<Shared>,
    Device(device): Device,
    Json(request): Json<TestRequest>,
) -> Result<Json<TestAnswer>, ApiError> {
    if state.apns.is_off() {
        return Err(ApiError::NoPush);
    }
    if !valid_test_id(&request.id) {
        return Err(ApiError::Invalid("that test id is not valid"));
    }
    let sent = alert(&state, &device, request.id)
        .await
        .ok_or(ApiError::Invalid(
            "this phone has not registered for alerts",
        ))?;
    let answer = match sent {
        Sent::Delivered => TestAnswer::Delivered,
        Sent::Gone => TestAnswer::Gone,
        Sent::Limited => TestAnswer::Limited,
        Sent::Refused { status } => TestAnswer::Refused { status },
        Sent::Unreachable => TestAnswer::Unreachable,
        Sent::Unsent => return Err(ApiError::Invalid("the test push could not be sealed")),
    };
    Ok(Json(answer))
}

/// Sends `device` one sealed test alert with `id` through the push relay,
/// as the phone's own test and Sheprd's (`Request::TestPush`) both ask,
/// and notes it in the ledger. `None` when the phone registered no alert
/// handle the push relay takes, so nothing went out. A handle APNs no
/// longer knows is forgotten, and the phone registers a new one when it
/// next opens.
pub(crate) async fn alert(state: &Shared, device: &DeviceId, id: String) -> Option<Sent> {
    let stored = state
        .auth()
        .devices()
        .get(device)
        .and_then(|record| record.apns.clone());
    let (token, key) = target(state, stored.as_ref())?;
    let badge = u32::try_from(state.attention().len()).unwrap_or(u32::MAX);
    let tag = format!("test-{id}");
    let sealed = Sealed::test(
        id,
        apns::computer_id(&state.fingerprint),
        &state.computer,
        badge,
    );
    let sent = state.apns.alert(&token, &key, sealed, &tag, now_ms()).await;
    state.pushes.record(apns::Kind::Alert, &sent, now_ms());
    match &sent {
        Sent::Gone => {
            let (state, device) = (state.clone(), device.clone());
            let _ = tokio::task::spawn_blocking(move || {
                crate::server::forget(&state, &device, &token);
            })
            .await;
        }
        Sent::Unreachable => {
            tracing::warn!(delivery = state.apns.name(), "could not send a test push");
        }
        _ => {}
    }
    Some(sent)
}

/// The alert handle and notify key in a device's stored registration
/// (`DeviceRecord::apns`), when the push relay takes that handle: a phone
/// with only Live Activities, or a plain token from an app older than the
/// push relay, has none.
pub(crate) fn target(
    state: &Shared,
    stored: Option<&serde_json::Value>,
) -> Option<(apns::Recipient, p256::PublicKey)> {
    Registration::from_stored(stored)
        .alerts()
        .filter(|(token, _)| state.apns.reaches(token))
        .map(|(token, key)| (token.clone(), *key))
}
