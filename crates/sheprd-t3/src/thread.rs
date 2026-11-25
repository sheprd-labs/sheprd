//! One thread: what `orchestration.subscribeThread` streams.
//!
//! The stream starts with the thread's projection at a sequence, then sends
//! each change as an event carrying the whole changed row, so applying an
//! event replaces the row by id. Events at or below the thread's sequence
//! are old and ignored; events of a type this client does not know still
//! move the sequence on, as T3's own clients do.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use sheprd_chat_protocol::EntryId;

/// A message's text, which a turn item may leave out to save repeating it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub streaming: bool,
}

/// A step of the conversation, in `ordinal` order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnItem {
    pub id: String,
    pub ordinal: u64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(flatten)]
    pub kind: TurnKind,
}

/// What a turn item is. Kinds Sheprd does not show are `Other`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnKind {
    #[serde(rename_all = "camelCase")]
    UserMessage {
        message_id: String,
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    AssistantMessage {
        message_id: String,
        #[serde(default)]
        text: Option<String>,
    },
    Reasoning {
        #[serde(default)]
        text: String,
    },
    ProposedPlan {
        #[serde(default)]
        markdown: String,
    },
    #[serde(rename_all = "camelCase")]
    CommandExecution {
        #[serde(default)]
        input: String,
        #[serde(default)]
        output: Option<String>,
        #[serde(default)]
        exit_code: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    DynamicTool {
        #[serde(default)]
        tool_name: Option<String>,
        #[serde(default)]
        input: Value,
    },
    #[serde(rename_all = "camelCase")]
    FileChange { file_name: String },
    WebSearch {
        #[serde(default)]
        patterns: Option<Vec<String>>,
    },
    /// The agent asks to go ahead (`orchestrationV2.ts:1493`); answered
    /// with one of `options`' decisions, or any of T3's when it names none.
    #[serde(rename_all = "camelCase")]
    ApprovalRequest {
        #[serde(default)]
        options: Vec<ApprovalOption>,
        #[serde(default)]
        prompt: Option<String>,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        request_id: Option<String>,
    },
    /// The agent asks questions (`orchestrationV2.ts:1452`).
    #[serde(rename_all = "camelCase")]
    UserInputRequest {
        #[serde(default)]
        questions: Vec<InputQuestion>,
        #[serde(default)]
        request_id: Option<String>,
    },
    SystemNotice {
        #[serde(default)]
        message: String,
    },
    Error {
        #[serde(default)]
        failure: Value,
    },
    #[serde(rename_all = "camelCase")]
    Handoff {
        #[serde(default)]
        to_model: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// One way to answer an approval request (`providerPolicy.ts:63`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ApprovalOption {
    pub decision: String,
    pub label: String,
}

/// A question of a user input request (`orchestrationV2.ts:1196`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputQuestion {
    pub id: String,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub options: Vec<InputOption>,
    pub question: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct InputOption {
    #[serde(default)]
    pub description: Option<String>,
    pub label: String,
    /// What the answer carries when it differs from the label.
    #[serde(default)]
    pub value: Option<String>,
}

/// The provider instance and model a thread runs (`modelSelection.ts:9`).
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub instance_id: String,
    pub model: String,
}

/// The parts of the projection's thread row Sheprd shows.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub model_selection: Option<ModelSelection>,
    #[serde(default)]
    pub provider_instance_id: Option<String>,
}

/// A run's id, place, and status: whether the agent is working.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct Run {
    id: String,
    #[serde(default)]
    ordinal: u64,
    status: String,
}

impl Run {
    fn active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "preparing" | "queued" | "starting" | "running" | "waiting"
        )
    }
}

/// A runtime request's id and status: whether the agent waits on the user.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct Request {
    id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Projection {
    #[serde(default)]
    thread: Header,
    #[serde(default)]
    messages: Vec<Message>,
    #[serde(default)]
    turn_items: Vec<Value>,
    #[serde(default)]
    runs: Vec<Run>,
    #[serde(default)]
    runtime_requests: Vec<Request>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
enum Item {
    #[serde(rename = "snapshot", rename_all = "camelCase")]
    Snapshot {
        snapshot_sequence: u64,
        projection: Projection,
    },
    #[serde(rename = "event")]
    Event { sequence: u64, event: Event },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
}

/// A thread, kept current from its stream.
#[derive(Debug, Default)]
pub struct Thread {
    pub header: Header,
    /// Each turn item's entry id, given when it is first seen and kept, so a
    /// client's view of an entry survives later items arriving.
    entry_ids: HashMap<String, EntryId>,
    items: HashMap<String, TurnItem>,
    messages: HashMap<String, Message>,
    next_entry: u64,
    requests: HashMap<String, String>,
    runs: HashMap<String, Run>,
    /// The sequence the thread is current to; `None` before the snapshot.
    pub sequence: Option<u64>,
}

impl Thread {
    /// Applies one stream value; values this version cannot read are skipped.
    pub fn apply(&mut self, value: Value) {
        let Ok(item) = serde_json::from_value::<Item>(value) else {
            return;
        };
        match item {
            Item::Snapshot {
                snapshot_sequence,
                projection,
            } => self.replace(snapshot_sequence, projection),
            Item::Event { sequence, event } => {
                if self.sequence.is_some_and(|current| sequence <= current) {
                    return;
                }
                self.sequence = Some(sequence);
                self.event(event);
            }
            Item::Other => {}
        }
    }

    fn replace(&mut self, sequence: u64, projection: Projection) {
        self.sequence = Some(sequence);
        self.header = projection.thread;
        self.messages = projection
            .messages
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect();
        self.runs = projection
            .runs
            .into_iter()
            .map(|run| (run.id.clone(), run))
            .collect();
        self.requests = projection
            .runtime_requests
            .into_iter()
            .map(|request| (request.id, request.status))
            .collect();
        self.items.clear();
        let mut items: Vec<TurnItem> = projection
            .turn_items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect();
        items.sort_by_key(|item| item.ordinal);
        for item in items {
            self.upsert(item);
        }
    }

    fn event(&mut self, event: Event) {
        match event.kind.as_str() {
            "message.updated" => {
                if let Ok(message) = serde_json::from_value::<Message>(event.payload) {
                    self.messages.insert(message.id.clone(), message);
                }
            }
            "turn-item.updated" => {
                if let Ok(item) = serde_json::from_value::<TurnItem>(event.payload) {
                    self.upsert(item);
                }
            }
            "run.created" | "run.updated" => {
                if let Ok(run) = serde_json::from_value::<Run>(event.payload) {
                    self.runs.insert(run.id.clone(), run);
                }
            }
            // Renames and model, provider, and mode changes carry the
            // whole thread row (`orchestrationV2.ts:1670-1699`).
            kind if kind.starts_with("thread.") => {
                if let Ok(header) = serde_json::from_value::<Header>(event.payload)
                    && header.id == self.header.id
                {
                    self.header = header;
                }
            }
            "runtime-request.updated" => {
                if let Ok(request) = serde_json::from_value::<Request>(event.payload) {
                    self.requests.insert(request.id, request.status);
                }
            }
            _ => {}
        }
    }

    fn upsert(&mut self, item: TurnItem) {
        if !self.entry_ids.contains_key(&item.id) {
            self.next_entry += 1;
            self.entry_ids
                .insert(item.id.clone(), EntryId(self.next_entry));
        }
        self.items.insert(item.id.clone(), item);
    }

    /// The turn items in conversation order, each with its entry id.
    pub fn items(&self) -> Vec<(EntryId, &TurnItem)> {
        let mut items: Vec<(EntryId, &TurnItem)> = self
            .items
            .values()
            .filter_map(|item| Some((*self.entry_ids.get(&item.id)?, item)))
            .collect();
        items.sort_by_key(|(_, item)| item.ordinal);
        items
    }

    pub fn message(&self, id: &str) -> Option<&Message> {
        self.messages.get(id)
    }

    /// Whether a run is under way.
    pub fn working(&self) -> bool {
        self.runs.values().any(Run::active)
    }

    /// The latest run under way, which an interrupt stops; queued runs
    /// have not started, so they are not it (as T3's own client picks).
    pub fn active_run(&self) -> Option<&str> {
        self.runs
            .values()
            .filter(|run| run.active() && run.status != "queued")
            .max_by_key(|run| run.ordinal)
            .map(|run| run.id.as_str())
    }

    /// Whether the agent waits on the user to answer a request.
    pub fn waiting(&self) -> bool {
        self.requests.values().any(|status| status == "pending")
    }

    /// Whether the runtime request `id` still waits for an answer.
    pub fn pending(&self, id: &str) -> bool {
        self.requests
            .get(id)
            .is_some_and(|status| status == "pending")
    }

    /// The turn item shown as entry `id`.
    pub fn item(&self, id: EntryId) -> Option<&TurnItem> {
        self.entry_ids
            .iter()
            .find(|(_, entry)| **entry == id)
            .and_then(|(item, _)| self.items.get(item))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
