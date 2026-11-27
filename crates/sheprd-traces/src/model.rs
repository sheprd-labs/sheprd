//! What the index keeps: sessions with their totals, the bounded events of
//! each, and per-day activity for charts.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sheprd_client::herdr::AgentKind;

/// What one event of a session is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A person's prompt.
    User,
    /// The agent's reply text.
    Agent,
    /// The agent's reasoning or its summary.
    Thinking,
    /// A tool the agent called, with its input.
    ToolCall,
    /// What a tool returned.
    ToolResult,
    /// A failed tool call or an error the agent reported.
    Error,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Thinking => "thinking",
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::Error => "error",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "user" => Self::User,
            "agent" => Self::Agent,
            "thinking" => Self::Thinking,
            "tool_call" => Self::ToolCall,
            "tool_result" => Self::ToolResult,
            "error" => Self::Error,
            _ => return None,
        })
    }
}

/// Publication visibility, independent of replication between authenticated
/// connected hosts. The index records `Private` for everything it adds;
/// public/link publishing and visibility controls are not implemented yet.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Private,
    Link,
    Public,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Link => "link",
            Self::Public => "public",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "private" => Self::Private,
            "link" => Self::Link,
            "public" => Self::Public,
            _ => return None,
        })
    }
}

/// One bounded, redacted event. `seq` orders events within their session
/// and stays the same when the file is read again, so adding it twice is a
/// no-op.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub seq: i64,
    /// Unix milliseconds, when the record carried a time.
    pub at: Option<i64>,
    pub kind: EventKind,
    pub tool: Option<String>,
    pub text: String,
}

/// One indexed session with its totals.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub host_id: String,
    pub host: String,
    #[serde(serialize_with = "agent_name", deserialize_with = "parse_agent")]
    pub agent: AgentKind,
    pub native_id: String,
    pub user: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub agent_version: Option<String>,
    pub started_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub messages: i64,
    pub user_messages: i64,
    pub tool_calls: i64,
    pub errors: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub visibility: Visibility,
}

/// Activity of one agent on one UTC day.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Activity {
    /// `YYYY-MM-DD`, UTC.
    pub day: String,
    #[serde(serialize_with = "agent_name")]
    pub agent: AgentKind,
    pub sessions: i64,
    pub messages: i64,
}

/// Distinct recently active sessions of one agent type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActiveAgent {
    #[serde(serialize_with = "agent_name")]
    pub agent: AgentKind,
    pub count: usize,
}

impl ActiveAgent {
    pub fn counts(agents: impl IntoIterator<Item = AgentKind>) -> Vec<Self> {
        let mut result: Vec<Self> = Vec::new();
        for agent in agents {
            if let Some(entry) = result.iter_mut().find(|entry| entry.agent == agent) {
                entry.count += 1;
            } else {
                result.push(Self { agent, count: 1 });
            }
        }
        result.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.agent.name().cmp(b.agent.name()))
        });
        result
    }
}

fn agent_name<S: Serializer>(agent: &AgentKind, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(agent.name())
}

fn parse_agent<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AgentKind, D::Error> {
    let name = String::deserialize(deserializer)?;
    AgentKind::parse(&name).ok_or_else(|| serde::de::Error::custom("unknown trace agent"))
}
