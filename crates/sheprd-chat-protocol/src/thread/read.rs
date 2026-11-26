//! What an agent reads of its thread through `thread_read` and
//! `thread_search` (`sheprd-acp thread-mcp`): entries of the transcript
//! file by id, each as the handoff quotes it but whole, and the ids whose
//! text matches a query. Ids the file trimmed answer "trimmed", never
//! nothing.
//!
//! Each entry's first line starts with its id, `#12 User: …`; its further
//! lines start with [`crate::handoff::QUOTED`], so text inside an entry
//! cannot pass for another entry.

use std::fmt::Write as _;

use super::file::Contents;
use crate::{EntryId, handoff};

/// The most entries one read returns.
pub const MAX_ENTRIES: usize = 64;
/// The most text one read returns, counted as JSON escapes it: within the
/// MCP server's 256 KiB message bound, with room for the envelope.
pub const MAX_BYTES: usize = 160 * 1024;
/// The most ids one search returns, newest first.
pub const MAX_HITS: usize = 64;
/// The longest line a search hit shows, in bytes.
pub const EXCERPT: usize = 200;

/// One read's answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Read {
    /// How many entries it holds.
    pub entries: usize,
    /// The first id not returned for lack of room, to read on from.
    pub next: Option<EntryId>,
    pub text: String,
}

/// The entries from `from` through `to` that `contents` holds, oldest
/// first, and the trimmed runs among them, within [`MAX_ENTRIES`] and
/// [`MAX_BYTES`].
pub fn read(contents: &Contents, from: EntryId, to: EntryId) -> Read {
    // Kept entries and trimmed runs never overlap, so ordering them by
    // their first id puts each run where it belongs.
    // Only ids are gathered first; an entry's text is made only once the
    // read reaches it.
    let mut items: Vec<(EntryId, Option<&crate::Entry>)> = contents
        .trimmed
        .iter()
        .filter(|range| range.to >= from && range.from <= to)
        .map(|range| (range.from.max(from), None))
        .collect();
    items.extend(
        contents
            .entries
            .iter()
            .filter(|entry| (from..=to).contains(&entry.id))
            .map(|entry| (entry.id, Some(entry))),
    );
    items.sort_by_key(|(id, _)| *id);
    let mut text = String::new();
    let mut used = 0;
    let mut entries = 0;
    let mut next = None;
    for (id, entry) in items {
        let block = match entry {
            Some(entry) => match handoff::full_line(&entry.kind) {
                Some(line) => block(id, &line),
                None => continue,
            },
            None => trimmed_block(contents, id, to),
        };
        let size = escaped_len(&block);
        let full = entry.is_some() && entries == MAX_ENTRIES;
        if full || (!text.is_empty() && used + size > MAX_BYTES) {
            next = Some(id);
            break;
        }
        let block = cut(block, MAX_BYTES.saturating_sub(used), " [cut]\n");
        used += escaped_len(&block);
        text.push_str(&block);
        entries += usize::from(entry.is_some());
    }
    match next {
        Some(id) => {
            let _ = writeln!(text, "[More: read on from #{}]", id.0);
        }
        None if text.is_empty() => {
            let _ = writeln!(text, "[No entries from #{} to #{}]", from.0, to.0);
        }
        None => {}
    }
    Read {
        entries,
        next,
        text,
    }
}

/// The ids of entries whose text holds `query`, ignoring case, newest
/// first, at most [`MAX_HITS`], each with the line that holds it.
pub fn search(contents: &Contents, query: &str) -> Vec<(EntryId, String)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    contents
        .entries
        .iter()
        .rev()
        .filter_map(|entry| {
            let text = handoff::full_line(&entry.kind)?;
            let line = text
                .lines()
                .find(|line| line.to_lowercase().contains(&query))?;
            Some((entry.id, cut(line.trim().to_owned(), EXCERPT, "…")))
        })
        .take(MAX_HITS)
        .collect()
}

/// The line for the trimmed run that covers `from`, up to `to` at most.
fn trimmed_block(contents: &Contents, from: EntryId, to: EntryId) -> String {
    let end = contents
        .trimmed
        .iter()
        .find(|range| range.contains(from))
        .map_or(from, |range| range.to.min(to));
    if from == end {
        format!("#{}: trimmed, no longer kept\n", from.0)
    } else {
        format!("#{}-#{}: trimmed, no longer kept\n", from.0, end.0)
    }
}

/// `line` as entry `id`'s block, its further lines quoted.
fn block(id: EntryId, line: &str) -> String {
    let mut block = format!("#{} ", id.0);
    for (at, part) in line.split('\n').enumerate() {
        if at > 0 {
            block.push_str(handoff::QUOTED);
        }
        block.push_str(part.strip_suffix('\r').unwrap_or(part));
        block.push('\n');
    }
    block
}

/// `text` cut to `max` bytes as JSON escapes it, on a character boundary,
/// ending with `mark` when cut.
fn cut(text: String, max: usize, mark: &str) -> String {
    if escaped_len(&text) <= max {
        return text;
    }
    let room = max.saturating_sub(escaped_len(mark));
    let mut used = 0;
    let mut end = 0;
    for (at, character) in text.char_indices() {
        used += escaped(character);
        if used > room {
            break;
        }
        end = at + character.len_utf8();
    }
    format!("{}{mark}", &text[..end])
}

/// What `text` weighs inside a JSON string.
fn escaped_len(text: &str) -> usize {
    text.chars().map(escaped).sum()
}

/// What `character` weighs inside a JSON string.
fn escaped(character: char) -> usize {
    match character {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
        '\0'..='\u{1f}' => 6,
        other => other.len_utf8(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
