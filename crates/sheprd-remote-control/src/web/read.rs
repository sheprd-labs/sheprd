//! `POST /api/v2/read`, a phone's way to mark chats read or unread by hand
//! (`control::read`). Home marks each unread row, so a change reaches every
//! phone connected, and Sheprd hears of it on its next read.

use super::{ApiError, Device, Shared};
use crate::control::read::{Change, Unread};
use axum::{Json, extract::State};

pub(super) async fn post(
    State(state): State<Shared>,
    _device: Device,
    Json(change): Json<Change>,
) -> Result<Json<Unread>, ApiError> {
    if !change.valid() {
        return Err(ApiError::Invalid(
            "a read change names one to 1024 pane ids",
        ));
    }
    Ok(Json(state.mark_read(&change)))
}
