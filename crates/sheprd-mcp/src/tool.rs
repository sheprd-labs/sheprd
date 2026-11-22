//! Tools as MCP describes them, the calls a client makes, and the
//! [`Handler`] that answers both.

use serde::Serialize;
use serde_json::{Map, Value};

/// The longest client name kept, in bytes. A client chooses it, and it is
/// shown to people in dialogs an MCP call opens.
pub const MAX_CLIENT_NAME: usize = 64;

/// What a server offers: its tools and what calling one does. Each binary
/// serving MCP implements it once; tests implement it to drive the server.
pub trait Handler {
    /// Guidance `initialize` gives the client's model, such as how to turn
    /// the tools on while they are off.
    fn instructions(&mut self) -> Option<String> {
        None
    }

    /// The tools to list now. The list may change between calls, but this
    /// server does not announce it, so clients read it when they connect.
    fn tools(&mut self) -> Vec<Tool>;

    /// Runs a tool. A failure the model can act on, including arguments
    /// that do not fit the tool's schema, is a [`ToolResult::error`]; only a
    /// name with no tool is [`UnknownTool`].
    fn call(&mut self, call: Call) -> Result<ToolResult, UnknownTool>;
}

/// No tool has the name a call gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownTool;

/// The client that connected, as its `initialize` named it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientInfo {
    /// Printable, and at most [`MAX_CLIENT_NAME`] bytes.
    pub name: String,
}

impl ClientInfo {
    /// Keeps what can be shown of a name a client sent: no control
    /// characters, cut at a character boundary within the bound.
    pub fn new(name: &str) -> Self {
        let mut kept = String::new();
        for character in name
            .trim()
            .chars()
            .filter(|character| !character.is_control())
        {
            if kept.len() + character.len_utf8() > MAX_CLIENT_NAME {
                break;
            }
            kept.push(character);
        }
        Self { name: kept }
    }
}

/// One `tools/call`.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub name: String,
    /// The call's arguments; empty when it gave none.
    pub arguments: Map<String, Value>,
    /// The client that asked, when its `initialize` named one.
    pub client: Option<ClientInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    /// A JSON Schema object for the arguments.
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// Hints for clients deciding whether to ask before a call. They are not
/// enforced, and clients must not trust them from servers they do not trust.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text { text: String },
}

/// What a call returned, success or a failure the model should see.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Vec<Content>,
    /// A JSON object with the same answer, for clients that read data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text { text: text.into() }],
            structured_content: None,
            is_error: false,
        }
    }

    /// An answer as text and as data. The text is what older clients show,
    /// so it says the same as the data.
    pub fn structured(text: impl Into<String>, data: Map<String, Value>) -> Self {
        Self {
            structured_content: Some(data),
            ..Self::text(text)
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            is_error: true,
            ..Self::text(text)
        }
    }
}
