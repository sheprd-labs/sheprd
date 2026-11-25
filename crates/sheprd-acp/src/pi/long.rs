//! pi's longest lines, read for only what the bridge uses.
//!
//! `agent_end` repeats every message of the run, screenshots and all, and
//! `message_end` and `turn_end` repeat a message with its tool results. Of a
//! message the bridge reads its role, why it stopped, its error and its token
//! usage, and it never reads `toolResults`, so the rest of each message, and
//! every tool result, is passed over as it streams rather than kept.
//! Everything else on the line is kept whole.

use std::{fmt, io::Read};

use serde::{
    Deserialize, Deserializer,
    de::{IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Value};

/// The fields of a message the bridge reads, in `Bridge::event` and
/// `Bridge::finished`.
const READ: [&str; 4] = ["role", "stopReason", "errorMessage", "usage"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    #[serde(default)]
    message: Option<Kept>,
    #[serde(default)]
    messages: Option<Kept>,
    /// `turn_end`'s, which the bridge never reads: passed over unkept.
    #[serde(default, rename = "toolResults")]
    _tool_results: Option<IgnoredAny>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

/// One long line as the bridge needs it; `None` when it is not JSON.
pub(super) fn read(line: &mut dyn Read) -> Option<Value> {
    let Event {
        message,
        messages,
        _tool_results: _,
        mut rest,
    } = serde_json::from_reader(line).ok()?;
    for (key, kept) in [("message", message), ("messages", messages)] {
        if let Some(Kept(value)) = kept {
            rest.insert(key.to_owned(), value);
        }
    }
    Some(Value::Object(rest))
}

/// A message cut to [`READ`], or a list of them. Anything else is kept as
/// it came.
struct Kept(Value);

impl<'de> Deserialize<'de> for Kept {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(KeptVisitor).map(Kept)
    }
}

struct KeptVisitor;

impl<'de> Visitor<'de> for KeptVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a pi message or a list of them")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut kept = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if READ.contains(&key.as_str()) {
                kept.insert(key, map.next_value()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(Value::Object(kept))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut kept = Vec::new();
        while let Some(Kept(value)) = seq.next_element()? {
            kept.push(value);
        }
        Ok(Value::Array(kept))
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(value.into())
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests;
