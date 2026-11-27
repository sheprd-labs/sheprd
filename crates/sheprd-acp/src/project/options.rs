//! Session modes and settings as ACP reports them, projected for clients,
//! and a client's new setting value back as ACP expects it.

use acp::v1 as wire;
use sheprd_chat_protocol::{
    Choice, ConfigOption, Mode, Modes, OptionCategory, OptionKind, OptionValue,
};

use super::bounded;

pub(crate) fn modes(state: wire::SessionModeState) -> Modes {
    Modes {
        available: state
            .available_modes
            .into_iter()
            .map(|mode| Mode {
                description: mode.description,
                id: mode.id.0.to_string(),
                name: mode.name,
            })
            .collect(),
        current: state.current_mode_id.0.to_string(),
    }
}

/// Settings kept at most, and choices kept per setting.
const MAX_OPTIONS: usize = 64;
pub(super) const MAX_CHOICES: usize = 256;

/// The settings in an ACP `configOptions` array. One this program cannot
/// read, such as a kind from a newer protocol, is skipped rather than
/// failing the message it came in.
pub(crate) fn options(value: Option<&serde_json::Value>) -> Vec<ConfigOption> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| serde_json::from_value::<wire::SessionConfigOption>(item.clone()).ok())
        .filter_map(config_option)
        .take(MAX_OPTIONS)
        .collect()
}

fn config_option(option: wire::SessionConfigOption) -> Option<ConfigOption> {
    let kind = match option.kind {
        wire::SessionConfigKind::Select(select) => {
            let choice = |choice: wire::SessionConfigSelectOption, group: Option<&str>| Choice {
                description: choice.description.map(bounded),
                group: group.map(str::to_owned),
                id: choice.value.0.to_string(),
                name: bounded(choice.name),
            };
            let choices: Vec<Choice> = match select.options {
                wire::SessionConfigSelectOptions::Ungrouped(choices) => {
                    choices.into_iter().map(|c| choice(c, None)).collect()
                }
                wire::SessionConfigSelectOptions::Grouped(groups) => groups
                    .into_iter()
                    .flat_map(|group| {
                        let name = group.name;
                        group
                            .options
                            .into_iter()
                            .map(move |c| choice(c, Some(&name)))
                            .collect::<Vec<_>>()
                    })
                    .collect(),
                _ => return None,
            };
            OptionKind::Select {
                choices: choices.into_iter().take(MAX_CHOICES).collect(),
                current: select.current_value.0.to_string(),
            }
        }
        wire::SessionConfigKind::Boolean(toggle) => OptionKind::Toggle {
            current: toggle.current_value,
        },
        _ => return None,
    };
    Some(ConfigOption {
        category: match option.category {
            Some(wire::SessionConfigOptionCategory::Mode) => OptionCategory::Mode,
            Some(wire::SessionConfigOptionCategory::Model) => OptionCategory::Model,
            Some(wire::SessionConfigOptionCategory::ModelConfig) => OptionCategory::ModelConfig,
            Some(wire::SessionConfigOptionCategory::ThoughtLevel) => OptionCategory::ThoughtLevel,
            _ => OptionCategory::Other,
        },
        description: option.description.map(bounded),
        id: option.id.0.to_string(),
        kind,
        name: bounded(option.name),
    })
}

/// The ACP value for a setting's new value.
pub(crate) fn option_value(value: &OptionValue) -> wire::SessionConfigOptionValue {
    match value {
        OptionValue::Choice { id } => wire::SessionConfigOptionValue::value_id(id.clone()),
        OptionValue::Toggle { value } => wire::SessionConfigOptionValue::boolean(*value),
    }
}
