//! Steering: a prompt sent into the running turn, which the harness hands
//! its agent at the agent's next step instead of after the turn.
//!
//! A translator that can steer says so in its handshake. The session then
//! sends [`METHOD`] while a turn runs; the translator answers it once the
//! harness holds the prompt, or with an error when it could not take it. It
//! reports [`STEERED`] when the agent took the prompt into the conversation,
//! and [`UNSTEERED`] when the turn ended without it, so the session sends
//! it as the next prompt instead. A turn holding a steered prompt the agent
//! has not taken does not end until the agent took it or gave it back.

use acp::v1 as wire;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::Out;
use crate::rpc::Inbound;

/// The `_meta` key of the handshake's capabilities saying the translator
/// takes [`METHOD`].
pub(crate) const CAPABILITY: &str = "herdr.steer";
/// The session's request: a prompt for the running turn.
pub(crate) const METHOD: &str = "herdr/steer";
/// The translator's notification: the agent took the prompt.
pub(crate) const STEERED: &str = "herdr/steered";
/// The translator's notification: the turn ended without the prompt.
pub(crate) const UNSTEERED: &str = "herdr/unsteered";

/// A prompt for the running turn.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Steer {
    /// The prompt's command, which names it in the notifications.
    pub id: Uuid,
    pub prompt: Vec<wire::ContentBlock>,
    pub session_id: String,
}

/// Which prompt a notification is about.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Steered {
    pub id: Uuid,
    pub session_id: String,
}

/// Tells the session the agent took prompt `id`.
pub(crate) fn steered(session: &str, id: Uuid) -> Out {
    notify(STEERED, session, id)
}

/// Tells the session the turn ended without prompt `id`.
pub(crate) fn unsteered(session: &str, id: Uuid) -> Out {
    notify(UNSTEERED, session, id)
}

fn notify(method: &str, session: &str, id: Uuid) -> Out {
    let params = Steered {
        id,
        session_id: session.to_owned(),
    };
    Out::Session(Inbound::Notification {
        method: method.to_owned(),
        params: serde_json::to_value(params).unwrap_or(Value::Null),
    })
}
