//! Legacy ACP `models` (`SessionModelState`) projected as a model config option,
//! for ACP servers such as Gemini CLI that report `models` without `configOptions`.

use serde::Deserialize;
use sheprd_chat_protocol::{Choice, ConfigOption, OptionCategory, OptionKind};

use super::{MAX_CHOICES, bounded};

/// The synthetic config option id used when an agent advertises `models`
/// without a `configOptions` model selector.
pub(crate) const OPTION_ID: &str = "model";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelState {
    #[serde(default)]
    available_models: Vec<ModelInfo>,
    current_model_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    #[serde(default)]
    description: Option<String>,
    model_id: String,
    name: String,
}

/// Projects an ACP `models` object into a selectable Model [`ConfigOption`].
pub(crate) fn option(value: Option<&serde_json::Value>) -> Option<ConfigOption> {
    let state: ModelState = serde_json::from_value(value?.clone()).ok()?;
    let choices: Vec<Choice> = state
        .available_models
        .into_iter()
        .take(MAX_CHOICES)
        .map(|model| Choice {
            description: model.description.map(bounded),
            group: None,
            id: model.model_id,
            name: bounded(model.name),
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    Some(ConfigOption {
        category: OptionCategory::Model,
        description: None,
        id: OPTION_ID.to_owned(),
        kind: OptionKind::Select {
            choices,
            current: state.current_model_id,
        },
        name: "Model".to_owned(),
    })
}
