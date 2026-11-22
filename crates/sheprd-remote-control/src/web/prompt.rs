//! The question a terminal agent waits on, as a phone shows it: read from
//! the pane's screen (`sheprd_client::herdr::prompt`) and answered with the
//! keys a person would press there. Reading neither attaches nor resizes.
//! An answer names the whole question it was shown and its choice, and the
//! screen is read again first, so a question that changed meanwhile, even
//! one offering the same choices, is never answered blind.

use super::{ApiError, Device, Shared};
use crate::hub::ApiMethod;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sheprd_client::herdr::prompt::{self, Prompt};

/// The longest typed answer.
const MAX_TEXT: usize = 4 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PromptAnswer {
    /// `None` when the pane shows no menu.
    pub(crate) prompt: Option<Prompt>,
}

/// `GET /api/term/{pane}/prompt`.
pub(super) async fn get(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
) -> Result<Json<PromptAnswer>, ApiError> {
    super::terminal::known(&state.hub.layout(), &pane)?;
    Ok(Json(PromptAnswer {
        prompt: current(&state, &pane).await?,
    }))
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
pub(super) struct Choose {
    /// The question as the phone showed it, every line: another approval
    /// with the same choices has another command in it.
    pub(super) question: Vec<String>,
    /// The choice's number and label as the phone showed them.
    pub(super) number: u32,
    pub(super) label: String,
    /// The words for a choice that takes them.
    #[serde(default)]
    pub(super) text: Option<String>,
}

/// What answering takes: keys alone, or moving to a choice that takes
/// words, the words, then Enter.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plan {
    Keys(Vec<&'static str>),
    Words {
        moves: Vec<&'static str>,
        text: String,
    },
}

/// How to answer `prompt`, on screen now, with `request`; refused when the
/// question or the choice is not the one the phone showed.
pub(super) fn plan(prompt: &Prompt, request: Choose) -> Result<Plan, ApiError> {
    if prompt.question != request.question {
        return Err(ApiError::Invalid("the question changed; look again"));
    }
    let index = prompt
        .choices
        .iter()
        .position(|choice| choice.number == request.number && choice.label == request.label)
        .ok_or(ApiError::Invalid("the question changed; look again"))?;
    let mut keys = prompt::keys_to(prompt, index);
    if !prompt.choices[index].text {
        return Ok(Plan::Keys(keys));
    }
    let text = request
        .text
        .filter(|text| !text.trim().is_empty() && text.len() <= MAX_TEXT)
        .ok_or(ApiError::Invalid("this choice needs words"))?;
    keys.pop();
    Ok(Plan::Words { moves: keys, text })
}

/// `POST /api/term/{pane}/prompt`: takes one choice.
pub(super) async fn choose(
    State(state): State<Shared>,
    device: Device,
    Path(pane): Path<String>,
    Json(request): Json<Choose>,
) -> Result<Json<Value>, ApiError> {
    super::terminal::known(&state.hub.layout(), &pane)?;
    let prompt = current(&state, &pane)
        .await?
        .ok_or(ApiError::Invalid("the question was already answered"))?;
    match plan(&prompt, request)? {
        Plan::Keys(keys) => send_keys(&state, &pane, &keys).await?,
        Plan::Words { moves, text } => {
            send_keys(&state, &pane, &moves).await?;
            state
                .hub
                .call(
                    ApiMethod::PaneSendText,
                    json!({"pane_id": pane, "text": text}),
                )
                .await?;
            send_keys(&state, &pane, &["enter"]).await?;
        }
    }
    state.typed.typed_by(&pane, &device.0);
    Ok(Json(json!({})))
}

async fn send_keys(state: &Shared, pane: &str, keys: &[&str]) -> Result<(), ApiError> {
    if keys.is_empty() {
        return Ok(());
    }
    state
        .hub
        .call(
            ApiMethod::PaneSendKeys,
            json!({"pane_id": pane, "keys": keys}),
        )
        .await?;
    Ok(())
}

async fn current(state: &Shared, pane: &str) -> Result<Option<Prompt>, ApiError> {
    let read = state
        .hub
        .call(
            ApiMethod::PaneRead,
            json!({"pane_id": pane, "source": "visible", "format": "text"}),
        )
        .await?;
    Ok(prompt::read(
        read["read"]["text"].as_str().unwrap_or_default(),
    ))
}
