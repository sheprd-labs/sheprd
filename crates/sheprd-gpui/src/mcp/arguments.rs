//! A tool call's arguments, read once each into typed values: what the
//! schema promises is checked here too, as clients do not all validate, and
//! an argument no tool reads is refused rather than ignored.

use super::Error;
use serde_json::{Map, Value};
use std::ops::RangeInclusive;

/// The most agent kinds one fan-out names.
const MAX_AGENT_KINDS: usize = 16;
const AGENTS: &str = "a list of {\"kind\", \"count\"} objects";

pub(super) struct Arguments(Map<String, Value>);

impl Arguments {
    pub(super) fn new(arguments: Map<String, Value>) -> Self {
        Self(arguments)
    }

    /// A string, absent or null as `None`, at most `max` bytes.
    pub(super) fn text(&mut self, name: &'static str, max: usize) -> Result<Option<String>, Error> {
        match self.0.remove(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) if text.len() > max => Err(Error::TooLong { name, max }),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(_) => Err(Error::Type {
                name,
                expected: "a string",
            }),
        }
    }

    pub(super) fn required_text(
        &mut self,
        name: &'static str,
        max: usize,
    ) -> Result<String, Error> {
        self.text(name, max)?
            .filter(|text| !text.trim().is_empty())
            .ok_or(Error::Missing { name })
    }

    /// A whole number within `range`, absent or null as `None`.
    pub(super) fn number(
        &mut self,
        name: &'static str,
        range: RangeInclusive<u64>,
    ) -> Result<Option<u64>, Error> {
        let out_of_range = || Error::Range {
            name,
            min: *range.start(),
            max: *range.end(),
        };
        match self.0.remove(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(number)) => match number.as_u64() {
                Some(value) if range.contains(&value) => Ok(Some(value)),
                Some(_) => Err(out_of_range()),
                None if number.as_i64().is_some() => Err(out_of_range()),
                None => Err(Error::Type {
                    name,
                    expected: "a whole number",
                }),
            },
            Some(_) => Err(Error::Type {
                name,
                expected: "a whole number",
            }),
        }
    }

    pub(super) fn required_number(
        &mut self,
        name: &'static str,
        range: RangeInclusive<u64>,
    ) -> Result<u64, Error> {
        self.number(name, range)?.ok_or(Error::Missing { name })
    }

    /// A boolean, absent or null as `false`.
    pub(super) fn flag(&mut self, name: &'static str) -> Result<bool, Error> {
        Ok(self.optional_flag(name)?.unwrap_or(false))
    }

    /// A boolean, absent or null as `None`.
    pub(super) fn optional_flag(&mut self, name: &'static str) -> Result<Option<bool>, Error> {
        match self.0.remove(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(flag)) => Ok(Some(flag)),
            Some(_) => Err(Error::Type {
                name,
                expected: "true or false",
            }),
        }
    }

    /// Agents and how many of each, as `[{"kind": "claude", "count": 2}]`,
    /// as `fan-out --agent claude:2` gives them: kinds lowercased, counts
    /// from 1 to `max`, at most [`MAX_AGENT_KINDS`] entries.
    pub(super) fn agents(
        &mut self,
        name: &'static str,
        max: u64,
    ) -> Result<Vec<(String, u8)>, Error> {
        let entries = match self.0.remove(name) {
            None | Some(Value::Null) => return Ok(Vec::new()),
            Some(Value::Array(entries)) if entries.len() <= MAX_AGENT_KINDS => entries,
            Some(Value::Array(_)) => {
                return Err(Error::Range {
                    name,
                    min: 0,
                    max: MAX_AGENT_KINDS as u64,
                });
            }
            Some(_) => {
                return Err(Error::Type {
                    name,
                    expected: AGENTS,
                });
            }
        };
        entries
            .into_iter()
            .map(|entry| {
                let Value::Object(entry) = entry else {
                    return Err(Error::Type {
                        name,
                        expected: AGENTS,
                    });
                };
                let mut entry = Self::new(entry);
                let kind = entry.required_text("kind", 64)?.to_ascii_lowercase();
                let count = entry.number("count", 1..=max)?.unwrap_or(1);
                entry.finish()?;
                Ok((kind, u8::try_from(count).unwrap_or(u8::MAX)))
            })
            .collect()
    }

    /// One of `choices`, as `parse` reads it; `listed` names them for the
    /// error.
    pub(super) fn choice<T>(
        &mut self,
        name: &'static str,
        listed: &'static str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> Result<Option<T>, Error> {
        let Some(text) = self.text(name, 64)? else {
            return Ok(None);
        };
        parse(&text).map(Some).ok_or(Error::Choice {
            name,
            choices: listed,
        })
    }

    /// Refuses any argument left unread, naming the first, cut short.
    pub(super) fn finish(self) -> Result<(), Error> {
        match self.0.into_iter().next() {
            None => Ok(()),
            Some((name, _)) => Err(Error::Unknown {
                name: sheprd_mcp::ClientInfo::new(&name.chars().take(64).collect::<String>()).name,
            }),
        }
    }
}
