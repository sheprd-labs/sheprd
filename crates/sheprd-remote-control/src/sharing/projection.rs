//! An allowlist projection, rebuilt from the host's reduced snapshot. Reconnects
//! therefore use exactly the same filter as live updates, including mutable tools.

use sheprd_chat_protocol::{
    ATTACHED, Change, Cursor, Entry, EntryId, EntryKind, MAX_ATTACHMENTS, Status, Tool,
    ToolContent, Transcript,
    sharing::{MAX_RESPONSE_BYTES, Update},
};

use crate::sharing::{Content, Error, Result};

const MAX_ENTRIES: usize = 256;
const MAX_TEXT: usize = 16 * 1024;
// Leave room for response framing and a whole update's delta overhead.
const ENTRY_BUDGET: usize = MAX_RESPONSE_BYTES / 2;

#[derive(Clone, Debug)]
pub(crate) struct Projection {
    ids: Vec<(EntryId, EntryId)>,
    next_id: u64,
    cursor: Cursor,
    transcript: Transcript,
    initialized: bool,
}

impl Projection {
    pub(crate) fn new() -> Self {
        Self {
            ids: Vec::new(),
            next_id: 1,
            cursor: Cursor::new_epoch(),
            transcript: Transcript::default(),
            initialized: false,
        }
    }

    pub(crate) fn read(
        &mut self,
        source: &Transcript,
        boundary: Option<EntryId>,
        content: Content,
        resume: Option<Cursor>,
    ) -> Result<Update> {
        let mut next = self.clone();
        let update = next.project(source, boundary, content, resume)?;
        *self = next;
        Ok(update)
    }

    fn project(
        &mut self,
        source: &Transcript,
        boundary: Option<EntryId>,
        content: Content,
        resume: Option<Cursor>,
    ) -> Result<Update> {
        validate(source)?;
        let (selected, truncated) = entries(source, boundary, content)?;
        // An entry that was hidden can become displayable after replacement.
        // Reset rather than inserting a new public ID before an existing one.
        let reset = selected.iter().any(|(id, _)| {
            self.ids.last().is_some_and(|(last, _)| id <= last)
                && !self.ids.iter().any(|(old, _)| old == id)
        });
        if reset {
            self.ids.clear();
            self.next_id = 1;
            self.cursor = Cursor::new_epoch();
            self.initialized = false;
        }
        let previous = self.cursor;
        let mut ids = Vec::new();
        let mut projected = Vec::new();
        for (source_id, kind) in selected {
            let id = match self.ids.iter().find(|(old, _)| *old == source_id) {
                Some((_, public)) => *public,
                None => {
                    let id = EntryId(self.next_id);
                    self.next_id = self.next_id.checked_add(1).ok_or(Error::Limit)?;
                    id
                }
            };
            let kind = match kind {
                EntryKind::Tool(mut tool) => {
                    tool.call_id = format!("shared-{}", id.0);
                    EntryKind::Tool(tool)
                }
                other => other,
            };
            ids.push((source_id, id));
            projected.push(Entry { id, kind });
        }
        let transcript = Transcript {
            agent: text(&source.agent, 128),
            entries: projected,
            read_only: true,
            status: match &source.status {
                Status::Starting => Status::Starting,
                Status::Idle => Status::Idle,
                Status::Working => Status::Working,
                Status::Failed { .. } => Status::Failed {
                    message: String::new(),
                },
                Status::Exited { .. } => Status::Exited {
                    message: String::new(),
                },
            },
            truncated,
            ..Transcript::default()
        };
        let changed = !self.initialized || self.transcript != transcript;
        let update = if !changed && resume == Some(self.cursor) {
            Update::Unchanged {
                cursor: self.cursor,
            }
        } else {
            if changed {
                self.cursor.seq = self.cursor.seq.checked_add(1).ok_or(Error::Limit)?;
            }
            let changes = (self.initialized && resume == Some(previous))
                .then(|| delta(&self.transcript, &transcript))
                .flatten();
            match changes {
                Some(changes) => Update::Changes {
                    cursor: self.cursor,
                    changes,
                },
                None => Update::Snapshot {
                    cursor: self.cursor,
                    transcript: Box::new(transcript.clone()),
                },
            }
        };
        // Bound the actual encoding, not only source text lengths. No partial
        // update is published if a future field grows beyond the wire budget.
        if serde_json::to_vec(&update)?.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Limit);
        }
        self.ids = ids;
        self.transcript = transcript;
        self.initialized = true;
        Ok(update)
    }
}

pub(crate) fn validate(source: &Transcript) -> Result<()> {
    if source
        .entries
        .windows(2)
        .any(|pair| pair[0].id >= pair[1].id)
    {
        return Err(Error::Source);
    }
    Ok(())
}

fn entries(
    source: &Transcript,
    boundary: Option<EntryId>,
    content: Content,
) -> Result<(Vec<(EntryId, EntryKind)>, bool)> {
    let mut selected = Vec::new();
    let mut bytes = 0;
    let mut truncated = source.truncated;
    for entry in source.entries.iter().rev() {
        if boundary.is_some_and(|boundary| entry.id <= boundary) {
            break;
        }
        let Some(kind) = filter(&entry.kind, content) else {
            continue;
        };
        let size = serde_json::to_vec(&kind)?.len() + 128;
        if selected.len() == MAX_ENTRIES || bytes + size > ENTRY_BUDGET {
            truncated = true;
            break;
        }
        bytes += size;
        selected.push((entry.id, kind));
    }
    selected.reverse();
    Ok((selected, truncated))
}

fn filter(kind: &EntryKind, content: Content) -> Option<EntryKind> {
    match kind {
        EntryKind::User {
            text: original,
            files,
            ..
        } => {
            if *files > MAX_ATTACHMENTS {
                return None;
            }
            let mut body = original.as_str();
            for _ in 0..*files {
                let (rest, attachment) = body.rsplit_once('\n').unwrap_or(("", body));
                if !attachment.starts_with(ATTACHED) {
                    return None;
                }
                body = rest;
            }
            Some(EntryKind::User {
                text: text(body, MAX_TEXT),
                files: 0,
                note: None,
            })
        }
        EntryKind::Agent { text: body } => Some(EntryKind::Agent {
            text: text(body, MAX_TEXT),
        }),
        EntryKind::Tool(tool) if content.tools => {
            let mut remaining = MAX_TEXT;
            let mut visible = Vec::new();
            for item in &tool.content {
                if remaining == 0 || visible.len() == 16 {
                    break;
                }
                if let ToolContent::Text { text: body } = item {
                    let body = text(body, remaining);
                    remaining = remaining.saturating_sub(body.len());
                    visible.push(ToolContent::Text { text: body });
                }
            }
            Some(EntryKind::Tool(Tool {
                call_id: String::new(),
                content: visible,
                kind: tool.kind,
                locations: Vec::new(),
                parent: None,
                started_ms: None,
                status: tool.status,
                title: text(&tool.title, 256),
            }))
        }
        // Explicit exclusions, so a new content variant requires a decision.
        EntryKind::Thought { .. }
        | EntryKind::Image { .. }
        | EntryKind::Tool(_)
        | EntryKind::Turn(_)
        | EntryKind::Notice { .. }
        | EntryKind::Handoff(_) => None,
    }
}

fn text(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let end = value.floor_char_boundary(limit.saturating_sub(3));
    format!("{}…", &value[..end])
}

fn delta(old: &Transcript, new: &Transcript) -> Option<Vec<Change>> {
    if old.agent != new.agent || old.truncated != new.truncated {
        return None;
    }
    let before = new.entries.first().map(|e| e.id);
    let retained: Vec<_> = old
        .entries
        .iter()
        .filter(|e| before.is_some_and(|id| e.id >= id))
        .collect();
    // Removal in the middle or reordering requires a replacement snapshot.
    if retained.len() > new.entries.len()
        || retained.iter().zip(&new.entries).any(|(a, b)| a.id != b.id)
    {
        return None;
    }
    let mut changes = Vec::new();
    if retained.len() < old.entries.len() {
        let before = before.or_else(|| {
            old.entries
                .last()
                .and_then(|e| e.id.0.checked_add(1).map(EntryId))
        })?;
        changes.push(Change::Truncate { before });
        // The common reducer marks truncation; don't let it diverge from the snapshot.
        if !new.truncated {
            return None;
        }
    }
    for (index, entry) in new.entries.iter().enumerate() {
        match retained.get(index) {
            Some(old) if **old != *entry => changes.push(Change::Replace {
                entry: entry.clone(),
            }),
            None => changes.push(Change::Push {
                entry: entry.clone(),
            }),
            _ => {}
        }
    }
    if old.status != new.status {
        changes.push(Change::Status {
            status: new.status.clone(),
        });
    }
    Some(changes)
}
