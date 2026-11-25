//! What a client of an earlier protocol revision is sent instead of a change
//! it cannot decode, which would otherwise end its connection.
//!
//! Every change moves the client's cursor by one, and a client takes a
//! change only right after the one before it, so an unknown change cannot
//! simply be left out. It is replaced with one the client knows that alters
//! nothing: the attached-client count, as it already stands.
//!
//! An entry kind the client cannot decode is sent as the notice it stands
//! for, in snapshots and in the changes that carry entries.

use sheprd_chat_protocol::{Change, Entry, EntryKind, REVISION, ServerMessage, Transcript};

/// The revision that added threads: `Change::Thread`, `Transcript::thread`,
/// and `EntryKind::Handoff`.
const THREADS: u32 = 5;

#[derive(Debug)]
pub(crate) struct Downgrade {
    /// The attached-client count the client holds, known from its snapshot.
    clients: Option<u32>,
    revision: u32,
}

impl Downgrade {
    pub(crate) fn new(revision: u32) -> Self {
        Self {
            clients: None,
            revision,
        }
    }

    /// Whether the client reads everything this program sends.
    pub(crate) fn current(&self) -> bool {
        self.revision >= REVISION
    }

    /// `message` as the client can read it, or `None` when it cannot be put
    /// so, which ends the connection; the client then reconnects for a
    /// snapshot.
    pub(crate) fn message(&mut self, message: ServerMessage) -> Option<ServerMessage> {
        if self.current() {
            return Some(message);
        }
        match message {
            ServerMessage::Snapshot {
                cursor,
                mut transcript,
            } => {
                self.clients = Some(transcript.clients);
                if self.revision < THREADS {
                    before_threads(&mut transcript);
                }
                Some(ServerMessage::Snapshot { cursor, transcript })
            }
            ServerMessage::Change { change, cursor } => {
                let change = match change {
                    Change::Clients { count } => {
                        self.clients = Some(count);
                        Change::Clients { count }
                    }
                    change if self.revision < since(&change) => Change::Clients {
                        count: self.clients?,
                    },
                    Change::Push { entry } if self.revision < THREADS => Change::Push {
                        entry: entry_before_threads(entry),
                    },
                    Change::Replace { entry } if self.revision < THREADS => Change::Replace {
                        entry: entry_before_threads(entry),
                    },
                    change => change,
                };
                Some(ServerMessage::Change { change, cursor })
            }
            message => Some(message),
        }
    }
}

/// The revision that added `change`.
fn since(change: &Change) -> u32 {
    match change {
        Change::Activity { .. } => 1,
        Change::Harnesses { .. } => 2,
        Change::PullRequest { .. } => 3,
        Change::Brief { .. } => 4,
        Change::Thread { .. } => THREADS,
        _ => 0,
    }
}

/// `transcript` as a client before threads reads it.
fn before_threads(transcript: &mut Transcript) {
    transcript.thread = None;
    for entry in &mut transcript.entries {
        if let EntryKind::Handoff(handoff) = &entry.kind {
            entry.kind = handoff.as_notice();
        }
    }
}

fn entry_before_threads(entry: Entry) -> Entry {
    match &entry.kind {
        EntryKind::Handoff(handoff) => Entry {
            id: entry.id,
            kind: handoff.as_notice(),
        },
        _ => entry,
    }
}

#[cfg(test)]
mod tests;
