//! The settings a new chat starts with, copied from the chat whose agent
//! asked for it: `sheprd chat new` from inside a chat starts the same agent
//! with the same model, thinking, and permission mode.
//!
//! The client that starts the program puts them in [`SETTINGS_ENV`]; the
//! program applies them once the agent's new session opens. A program
//! before this one ignores the variable, and one that does not know a
//! setting skips it, so the chat still starts.

use serde::{Deserialize, Serialize};

use crate::{Agent, OptionCategory, OptionKind, OptionValue, Transcript};

/// The environment variable that carries [`Settings`] as JSON.
pub const SETTINGS_ENV: &str = "SHEPRD_CHAT_SETTINGS";

/// The most bytes [`Settings`] take encoded: an environment variable, not
/// a transcript.
pub const MAX_SETTINGS: usize = 4 * 1024;

/// What a chat runs with, to start another like it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The built-in agent they were read from, by [`Agent::name`]; another
    /// agent's setting ids mean nothing to it.
    pub agent: String,
    /// Permission requests are answered without asking.
    #[serde(default)]
    pub auto_accept: bool,
    /// The session mode's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Setting values, in the order to apply them: the model before the
    /// settings of the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Setting>,
}

/// One setting's value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub id: String,
    pub value: OptionValue,
}

/// The kinds of setting carried, in the order they are applied. Others,
/// whatever an agent means by them, stay as a new session has them.
const CARRIED: [OptionCategory; 4] = [
    OptionCategory::Model,
    OptionCategory::ModelConfig,
    OptionCategory::ThoughtLevel,
    OptionCategory::Mode,
];

impl Settings {
    /// What `transcript`'s chat, running `agent`, uses now.
    pub fn of(agent: Agent, transcript: &Transcript) -> Self {
        let options = CARRIED
            .iter()
            .flat_map(|category| {
                transcript
                    .options
                    .iter()
                    .filter(move |option| option.category == *category)
            })
            .map(|option| Setting {
                id: option.id.clone(),
                value: match &option.kind {
                    OptionKind::Select { current, .. } => OptionValue::Choice {
                        id: current.clone(),
                    },
                    OptionKind::Toggle { current } => OptionValue::Toggle { value: *current },
                },
            })
            .collect();
        Self {
            agent: agent.name().to_owned(),
            auto_accept: transcript.auto_accept,
            mode: transcript.modes.as_ref().map(|modes| modes.current.clone()),
            options,
        }
    }

    /// Whether they are for `agent`.
    pub fn is_for(&self, agent: Agent) -> bool {
        self.agent == agent.name()
    }

    /// The variable's value, refused past [`MAX_SETTINGS`].
    pub fn encode(&self) -> crate::Result<String> {
        let text = serde_json::to_string(self)?;
        if text.len() > MAX_SETTINGS {
            return Err(crate::Error::SettingsLimit { len: text.len() });
        }
        Ok(text)
    }

    /// The settings a variable's value holds.
    pub fn decode(text: &str) -> crate::Result<Self> {
        if text.len() > MAX_SETTINGS {
            return Err(crate::Error::SettingsLimit { len: text.len() });
        }
        Ok(serde_json::from_str(text)?)
    }
}
