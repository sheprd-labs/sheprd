//! The pull requests the agent worked on: found in its finished tool calls,
//! or added by `sheprd chat pr add`, and kept in the transcript every
//! client shows.

use std::collections::VecDeque;

use acp::v1 as wire;
use serde_json::Value;
use sheprd_chat_protocol::{
    Change, EntryKind, MAX_PULL_REQUESTS, PullRequest, PullRequestAction, Rejection,
};

use super::{Session, Update};

/// Calls whose command is kept; the oldest goes first.
const MAX_COMMANDS: usize = 64;
/// The bytes of one command kept.
const MAX_COMMAND: usize = 16 * 1024;

/// The whole command of each recent shell call, from the
/// `command` of its input: its title shows only the first line.
#[derive(Debug, Default)]
pub(super) struct Commands(VecDeque<(String, String)>);

impl Commands {
    /// Keeps the command a tool call or its update carries in its input.
    pub(super) fn keep(&mut self, update: &wire::SessionUpdate) {
        let (id, input) = match update {
            wire::SessionUpdate::ToolCall(call) => (&call.tool_call_id, call.raw_input.as_ref()),
            wire::SessionUpdate::ToolCallUpdate(update) => {
                (&update.tool_call_id, update.fields.raw_input.as_ref())
            }
            _ => return,
        };
        let Some(command) = input.and_then(command_of) else {
            return;
        };
        let id = id.0.to_string();
        self.0.retain(|(known, _)| *known != id);
        if self.0.len() == MAX_COMMANDS {
            self.0.pop_front();
        }
        self.0.push_back((id, command));
    }

    fn get(&self, call_id: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(id, _)| id == call_id)
            .map(|(_, command)| command.as_str())
    }
}

/// The pull requests the running turn's tool calls created or pushed to,
/// each listed once per call even when the transcript knows it already, so
/// whoever watches the stream (the phone's "finished" push) learns which
/// one the turn touched last: a later call touching the same pull request
/// lists it again, a repeated update of the same call does not. Emptied
/// when the person speaks; the oldest make room.
#[derive(Debug, Default)]
pub(super) struct TurnPulls(VecDeque<(String, PullRequest)>);

impl TurnPulls {
    /// Whether call `call` created or pushed to `pull` and has not listed
    /// it yet; if so, it is now listed.
    fn take(&mut self, call: &str, pull: &PullRequest) -> bool {
        let touched = pull.actions.iter().any(|action| {
            matches!(
                action,
                PullRequestAction::Created | PullRequestAction::Pushed
            )
        });
        if !touched
            || self
                .0
                .iter()
                .any(|(known_call, known)| known_call == call && known.is(pull))
        {
            return false;
        }
        if self.0.len() == MAX_COMMANDS {
            self.0.pop_front();
        }
        self.0.push_back((call.to_owned(), pull.clone()));
        true
    }
}

/// The command an input names: a string, or the words of an argument list
/// such as Codex's `["bash", "-lc", "…"]`, bounded.
fn command_of(input: &Value) -> Option<String> {
    let mut command = match input.get("command")? {
        Value::String(command) => command.clone(),
        Value::Array(words) => words
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    if command.len() > MAX_COMMAND {
        let mut end = MAX_COMMAND;
        while !command.is_char_boundary(end) {
            end -= 1;
        }
        command.truncate(end);
    }
    Some(command)
}

impl<R: FnMut(Update)> Session<R> {
    /// Adds the pull request `url` names, as the agent asked.
    pub(super) fn add_pull_request(&mut self, url: &str) -> Result<(), Rejection> {
        let pull = PullRequest::parse(url.trim()).ok_or(Rejection::InvalidPullRequest)?;
        if self.transcript.pull_request(&pull).is_some() {
            return Ok(());
        }
        if self.transcript.pull_requests.len() >= MAX_PULL_REQUESTS {
            return Err(Rejection::TooManyPullRequests);
        }
        self.emit(Change::PullRequest { pull_request: pull });
        Ok(())
    }

    /// Records the pull requests a tool call `change` finished worked on,
    /// unless the transcript already has each with all it did and this
    /// turn already listed it ([`TurnPulls`]). A full list takes no more.
    pub(super) fn find_pull_requests(&mut self, change: &Change) {
        let (Change::Push { entry } | Change::Replace { entry }) = change else {
            return;
        };
        if matches!(entry.kind, EntryKind::User { .. }) {
            self.turn_pulls.0.clear();
            return;
        }
        let EntryKind::Tool(tool) = &entry.kind else {
            return;
        };
        // A call's command stays kept after it ends, as a later update of
        // the same call reads it again; the oldest make room.
        let found = crate::pull_requests::found(tool, self.commands.get(&tool.call_id));
        for pull in found {
            let known = self.transcript.pull_request(&pull);
            let new = known.is_none_or(|known| {
                pull.actions
                    .iter()
                    .any(|action| !known.actions.contains(action))
            });
            let room = known.is_some() || self.transcript.pull_requests.len() < MAX_PULL_REQUESTS;
            let turn = room && self.turn_pulls.take(&tool.call_id, &pull);
            if (new || turn) && room {
                self.emit(Change::PullRequest { pull_request: pull });
            }
        }
    }
}
