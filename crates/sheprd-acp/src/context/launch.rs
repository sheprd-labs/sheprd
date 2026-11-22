//! What the agent is told about itself: that it runs in a Sheprd chat,
//! as which agent and with which settings, and so how to start another
//! agent like it. Told again when any of that changes, as when the person
//! picks another model or a terminal agent moved into this chat.

use std::path::Path;

use sheprd_chat_protocol::{OptionCategory, Transcript};

/// The settings of the chat the agent is told about, by the names the
/// chat shows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Running {
    pub(crate) mode: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) thinking: Option<String>,
}

impl Running {
    pub(crate) fn of(transcript: &Transcript) -> Self {
        let named = |category| {
            transcript
                .option(category)
                .map(sheprd_chat_protocol::ConfigOption::current_name)
        };
        let mode = transcript
            .modes
            .as_ref()
            .and_then(|modes| modes.current())
            .map(|mode| mode.name.clone())
            .or_else(|| named(OptionCategory::Mode));
        Self {
            mode,
            model: named(OptionCategory::Model),
            thinking: named(OptionCategory::ThoughtLevel),
        }
    }
}

/// What the agent is and how it starts another like itself with `sheprd`.
pub(super) fn guide(sheprd: &Path, agent: &str, running: &Running) -> String {
    let sheprd = super::quoted(sheprd);
    let mut setup = String::new();
    for (what, value) in [
        ("model", &running.model),
        ("thinking", &running.thinking),
        ("permission mode", &running.mode),
    ] {
        if let Some(value) = value {
            setup.push_str(&format!(", {what} {value}"));
        }
    }
    format!(
        "You are {agent} running in a Sheprd chat (`sheprd-acp`), not in {agent}'s own \
         terminal interface{setup}.\n\
         When the person asks you to start another agent, such as in a new Herdr workspace, \
         tab, or worktree, start a chat like this one unless they ask for another kind: \
         create the workspace or tab with `herdr`, then run `{sheprd} chat new --pane PANE` \
         with the id of a pane in it. With no agent named, that starts {agent} with this \
         chat's model, thinking, and permission mode. Name another agent \
         (`{sheprd} chat new codex --pane PANE`) only when asked to, and do not run an \
         agent's terminal command there instead.\n"
    )
}
