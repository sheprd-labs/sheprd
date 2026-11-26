//! Prompts steered into Claude's running turn. Claude Code takes a user
//! message that comes while it works at its next step, and echoes it, by
//! the uuid it was sent with, when it does. One that comes after its last
//! step starts another turn of its own, which the session's prompt waits
//! for, so the session sees one turn.

use acp::v1 as wire;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Bridge, Control};
use crate::harness::{
    Out, out,
    out::{failure, reply},
    steer,
};

impl Bridge {
    /// Sends the prompt to Claude while its turn runs.
    pub(super) fn steer(&mut self, id: wire::RequestId, params: Value) -> Vec<Out> {
        let Ok(request) = serde_json::from_value::<steer::Steer>(params) else {
            return vec![reply(id, Err(wire::Error::invalid_params()))];
        };
        if self.prompt.is_none() || self.interrupted {
            return vec![reply(id, Err(failure("No turn is running.")))];
        }
        let content: Vec<Value> = request
            .prompt
            .iter()
            .filter_map(super::super::content)
            .collect();
        self.steers.push(request.id);
        vec![
            Out::Harness(json!({
                "type": "user",
                "uuid": request.id.to_string(),
                "message": {"role": "user", "content": content},
                "parent_tool_use_id": null,
                "session_id": self.session,
            })),
            reply(id, Ok(json!({}))),
        ]
    }

    /// A user message Claude echoed as it took it: a steered prompt's tells
    /// the session; the others, such as the prompt that started the turn,
    /// say nothing new.
    pub(super) fn replayed(&mut self, envelope: &Value) -> Vec<Out> {
        let Some(id) = envelope
            .get("uuid")
            .and_then(Value::as_str)
            .and_then(|uuid| Uuid::parse_str(uuid).ok())
        else {
            return Vec::new();
        };
        let Some(index) = self.steers.iter().position(|steer| *steer == id) else {
            return Vec::new();
        };
        self.steers.remove(index);
        vec![steer::steered(&self.session, id)]
    }

    /// Withdraws the prompts Claude has not taken, as the turn is
    /// cancelled; one it took already is still echoed.
    pub(super) fn withdraw_steers(&mut self) -> Vec<Out> {
        let waiting = self.steers.clone();
        waiting
            .into_iter()
            .map(|id| {
                self.control(
                    json!({"subtype": "cancel_async_message", "message_uuid": id.to_string()}),
                    Control::Withdraw(id),
                )
            })
            .collect()
    }

    /// Claude's answer to a withdrawal: a withdrawn prompt is no longer
    /// waited for.
    pub(super) fn withdrawn(&mut self, id: Uuid, result: &Result<Value, String>) {
        let cancelled = result
            .as_ref()
            .is_ok_and(|response| response.get("cancelled") == Some(&Value::Bool(true)));
        if cancelled {
            self.steers.retain(|steer| *steer != id);
        }
    }

    /// Whether the turn that just ended is followed by one Claude runs for
    /// prompts it did not take in time, which the session's prompt waits
    /// for. A cancelled turn is not.
    pub(super) fn turn_continues(&mut self) -> bool {
        if self.interrupted {
            self.steers.clear();
            return false;
        }
        !self.steers.is_empty()
    }

    /// Says how the turn that ended went, as the session's prompt goes on
    /// in the next.
    pub(super) fn continued(&self, failed: Option<String>) -> Vec<Out> {
        failed
            .map(|message| out::notice(&self.session, wire::NoticeSeverity::Error, message))
            .into_iter()
            .collect()
    }
}
