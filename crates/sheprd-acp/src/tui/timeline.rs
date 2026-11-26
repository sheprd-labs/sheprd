//! The transcript as a timeline: a colored rail down the left of each run
//! of entries by one author, under a header naming who wrote them, and the
//! agent's rail closing on its turn summary.

use std::collections::HashMap;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use sheprd_chat_protocol::{Author, Entry, EntryId, EntryKind, thread::Thread};

use super::lines;

/// Columns the rail takes before an entry's own lines.
pub(crate) const RAIL: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Group {
    User,
    Agent,
}

impl Group {
    fn of(entry: &Entry) -> Self {
        match &entry.kind {
            EntryKind::User { .. }
            | EntryKind::Image {
                author: Author::User,
                ..
            } => Self::User,
            _ => Self::Agent,
        }
    }

    fn color(self) -> Color {
        match self {
            Self::User => Color::Cyan,
            Self::Agent => Color::Magenta,
        }
    }
}

/// Whether the entry draws at all: a sub-agent's steps show only in Sheprd.
fn shown(entry: &Entry) -> bool {
    !matches!(&entry.kind, EntryKind::Tool(tool) if tool.parent.is_some())
}

fn is_tool(entry: &Entry) -> bool {
    matches!(entry.kind, EntryKind::Tool(_))
}

/// One entry's lines at `width` columns, rail included.
fn railed(entry: &Entry, width: usize) -> Vec<Line<'static>> {
    let inner = lines::entry(entry, width.saturating_sub(RAIL));
    if matches!(entry.kind, EntryKind::Turn(_)) {
        // The summary's own corner closes the rail; its blank line has none.
        return inner
            .into_iter()
            .map(|line| {
                if line.spans.is_empty() {
                    return line;
                }
                let mut spans = vec![Span::raw(" ")];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect();
    }
    let rail = Style::default().fg(Group::of(entry).color());
    inner
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::styled(" │ ", rail)];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// The name of the agent that held the conversation at entry `id`: the
/// segment it falls in, or `current` for the segment holding it now and
/// for a chat that keeps no thread.
fn agent_at<'a>(thread: Option<&Thread>, current: &'a str, id: EntryId) -> &'a str {
    let Some(thread) = thread else {
        return current;
    };
    let last = thread.segments.len().saturating_sub(1);
    let index = thread
        .segments
        .iter()
        .rposition(|segment| segment.after.is_none_or(|after| after < id))
        .unwrap_or(0);
    match thread.segments.get(index) {
        Some(segment) if index < last => segment.agent.display_name(),
        _ => current,
    }
}

fn header(group: Group, agent: &str) -> Line<'static> {
    let style = Style::default()
        .fg(group.color())
        .add_modifier(Modifier::BOLD);
    let (mark, name) = match group {
        Group::User => (" ◆ ", "you".to_owned()),
        Group::Agent => (" ◇ ", agent.to_owned()),
    };
    Line::from(vec![Span::styled(mark, style), Span::styled(name, style)])
}

/// Every entry's lines, with a header where the author changes, naming the
/// agent of the `thread` segment each reply falls in. Entry lines
/// come from `cache`, filled at `width`; headers and spacing are cheap and
/// drawn fresh, since they depend on the neighbors.
pub(crate) fn lines(
    entries: &[Entry],
    thread: Option<&Thread>,
    agent: &str,
    width: usize,
    cache: &mut HashMap<EntryId, Vec<Line<'static>>>,
) -> Vec<Line<'static>> {
    let shown: Vec<&Entry> = entries.iter().filter(|entry| shown(entry)).collect();
    let mut out = Vec::new();
    let mut current = None;
    for (index, entry) in shown.iter().enumerate() {
        let turn = matches!(entry.kind, EntryKind::Turn(_));
        let group = Group::of(entry);
        let author = match group {
            Group::User => None,
            Group::Agent => Some(agent_at(thread, agent, entry.id)),
        };
        if !turn && current != Some((group, author)) {
            out.push(header(group, author.unwrap_or(agent)));
            current = Some((group, author));
        }
        if turn {
            current = None;
        }
        let lines = cache
            .entry(entry.id)
            .or_insert_with(|| railed(entry, width));
        // Steps in a row stay together; the blank line comes after the last.
        let together = is_tool(entry) && shown.get(index + 1).is_some_and(|next| is_tool(next));
        let take = lines.len() - usize::from(together && !lines.is_empty());
        out.extend(lines.iter().take(take).cloned());
    }
    out
}

#[cfg(test)]
mod tests;
