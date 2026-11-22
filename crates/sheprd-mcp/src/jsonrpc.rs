//! JSON-RPC 2.0 as MCP uses it: one object per line, request IDs that are
//! integers or strings and never null, and no batches.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::borrow::Cow;

// keep-sorted start
pub(crate) const INTERNAL_ERROR: i64 = -32603;
pub(crate) const INVALID_PARAMS: i64 = -32602;
pub(crate) const INVALID_REQUEST: i64 = -32600;
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const PARSE_ERROR: i64 = -32700;
/// Implementation-defined: too many requests wait for the one running.
pub(crate) const SERVER_BUSY: i64 = -32000;
// keep-sorted end

/// The longest string ID, in bytes. Every answer echoes its ID, so a
/// bounded one keeps answers within [`crate::MAX_MESSAGE`].
pub(crate) const MAX_ID: usize = 128;

/// A request ID, echoed in its response.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Id {
    Number(i64),
    String(String),
}

#[derive(Debug, PartialEq)]
pub(crate) struct Request {
    pub(crate) id: Id,
    pub(crate) method: String,
    pub(crate) params: Option<Map<String, Value>>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Incoming {
    Request(Request),
    Notification {
        method: String,
        params: Option<Map<String, Value>>,
    },
    /// A response to a request of ours. This server sends none, so it is
    /// dropped.
    Response,
}

/// A JSON-RPC error: its code and the message the client reads.
#[derive(Debug, PartialEq)]
pub(crate) struct Failure {
    pub(crate) code: i64,
    pub(crate) message: Cow<'static, str>,
}

impl Failure {
    pub(crate) fn new(code: i64, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// A message that cannot be served, and the error to answer it with.
#[derive(Debug, PartialEq)]
pub(crate) struct Refusal {
    /// The ID, when one could be read; null in the answer otherwise.
    pub(crate) id: Option<Id>,
    pub(crate) failure: Failure,
}

impl Refusal {
    fn new(id: Option<Id>, code: i64, message: &'static str) -> Self {
        Self {
            id,
            failure: Failure::new(code, message),
        }
    }
}

/// Reads one line's message.
pub(crate) fn decode(line: &[u8]) -> Result<Incoming, Refusal> {
    let value: Value =
        serde_json::from_slice(line).map_err(|_| Refusal::new(None, PARSE_ERROR, "Parse error"))?;
    let Value::Object(mut object) = value else {
        let message = if value.is_array() {
            "Batches are not supported"
        } else {
            "A message must be a JSON object"
        };
        return Err(Refusal::new(None, INVALID_REQUEST, message));
    };
    let id = match object.remove("id") {
        None => None,
        Some(value) => Some(serde_json::from_value::<Id>(value).map_err(|_| {
            Refusal::new(
                None,
                INVALID_REQUEST,
                "The id must be an integer or a string",
            )
        })?),
    };
    if matches!(&id, Some(Id::String(id)) if id.len() > MAX_ID) {
        return Err(Refusal::new(None, INVALID_REQUEST, "The id is too long"));
    }
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(Refusal::new(id, INVALID_REQUEST, "jsonrpc must be \"2.0\""));
    }
    let Some(method) = object.remove("method") else {
        if id.is_some() && (object.contains_key("result") || object.contains_key("error")) {
            return Ok(Incoming::Response);
        }
        return Err(Refusal::new(
            id,
            INVALID_REQUEST,
            "A request needs a method",
        ));
    };
    let Value::String(method) = method else {
        return Err(Refusal::new(
            id,
            INVALID_REQUEST,
            "The method must be a string",
        ));
    };
    let params = match object.remove("params") {
        None => None,
        Some(Value::Object(params)) => Some(params),
        Some(_) => return Err(Refusal::new(id, INVALID_PARAMS, "params must be an object")),
    };
    Ok(match id {
        Some(id) => Incoming::Request(Request { id, method, params }),
        None => Incoming::Notification { method, params },
    })
}

pub(crate) fn result(id: &Id, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub(crate) fn error(id: Option<&Id>, failure: &Failure) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": failure.code, "message": failure.message } })
}

impl From<&Refusal> for Value {
    fn from(refusal: &Refusal) -> Self {
        error(refusal.id.as_ref(), &refusal.failure)
    }
}
