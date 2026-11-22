//! The tools `sheprd mcp` offers, each declared once: its description,
//! schema, hints, and the command it becomes. A tool is a typed
//! [`ControlCommand`], so it runs the same code, with the same targeting,
//! bounds, and dialogs, as the `sheprd` command it is named after.

mod act;
mod read;

use super::{Error, arguments::Arguments};
use crate::cli::control::ControlCommand;
use serde_json::{Map, Value, json};
use sheprd_mcp::{Annotations, Tool};

/// What a tool does to the app or the world, for clients deciding whether
/// to ask the person first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Effect {
    /// Reads only.
    Reads,
    /// Shows something or keeps something, and can be undone or redone.
    Changes,
    /// Removes something, or opens a dialog where the person may.
    Removes,
}

pub(super) struct Spec {
    pub(super) name: &'static str,
    pub(super) title: &'static str,
    pub(super) description: &'static str,
    pub(super) effect: Effect,
    /// The arguments' JSON Schema properties, besides `window`.
    pub(super) properties: fn() -> Value,
    pub(super) required: &'static [&'static str],
    pub(super) build: fn(&mut Arguments) -> Result<ControlCommand, Error>,
}

/// Every tool, read-only ones first.
pub(super) fn all() -> impl Iterator<Item = &'static Spec> {
    read::TOOLS.iter().chain(act::TOOLS)
}

pub(super) fn find(name: &str) -> Option<&'static Spec> {
    all().find(|spec| spec.name == name)
}

impl Spec {
    pub(super) fn tool(&self) -> Tool {
        let mut properties = match (self.properties)() {
            Value::Object(properties) => properties,
            _ => Map::new(),
        };
        properties.insert(
            "window".into(),
            json!({
                "type": "integer",
                "minimum": 0,
                "description": "The window to act in, by the id window_list reports. Absent: the window showing the caller's workspace, or the frontmost one.",
            }),
        );
        Tool {
            name: self.name.into(),
            title: Some(self.title.into()),
            description: self.description.into(),
            input_schema: json!({
                "type": "object",
                "properties": properties,
                "required": self.required,
                "additionalProperties": false,
            }),
            annotations: Some(Annotations {
                read_only_hint: self.effect == Effect::Reads,
                destructive_hint: self.effect == Effect::Removes,
                idempotent_hint: self.effect == Effect::Reads,
                // Pages and pull requests reach beyond this computer.
                open_world_hint: true,
            }),
        }
    }
}

/// A tool taking no arguments.
fn none() -> Value {
    json!({})
}

fn integer(description: &str, min: u64, max: u64) -> Value {
    json!({ "type": "integer", "minimum": min, "maximum": max, "description": description })
}

fn string(description: &str, max: usize) -> Value {
    json!({ "type": "string", "maxLength": max, "description": description })
}
