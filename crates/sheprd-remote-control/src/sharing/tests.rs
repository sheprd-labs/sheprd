use std::time::Duration;

use sheprd_chat_protocol::{
    Cursor, EndpointId, Entry, EntryId, EntryKind, SessionInfo, Transcript,
    sharing::{Read, Update, VERSION},
};
use tempfile::TempDir;

use crate::sharing::{Clock, Content, Guest, History, Invitation, Source, Store};

mod admission;
mod projection;
mod storage;

struct Fixture {
    dir: TempDir,
    endpoint: EndpointId,
    cursor: Cursor,
    transcript: Transcript,
    store: Store,
}

fn clock(ms: u64) -> Clock {
    Clock {
        unix_ms: 100_000 + ms,
        elapsed: Duration::from_millis(ms),
    }
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("sharing"), clock(0)).unwrap();
        Self {
            dir,
            store,
            endpoint: EndpointId::random(),
            cursor: Cursor::new_epoch(),
            transcript: Transcript {
                agent: "Example agent".into(),
                session: Some(SessionInfo {
                    id: "stable-agent-session".into(),
                    cwd: Some("private-folder".into()),
                    title: Some("private-title".into()),
                    continued: None,
                }),
                ..Transcript::default()
            },
        }
    }

    fn share(&mut self, history: History, content: Content) -> (Invitation, Guest) {
        let source = Source {
            endpoint: &self.endpoint,
            cursor: self.cursor,
            transcript: &self.transcript,
        };
        let id = self
            .store
            .create(source, history, content, Duration::from_secs(60), clock(0))
            .unwrap();
        let invite = self.store.invite(id, clock(0)).unwrap();
        let guest = self
            .store
            .admit(id, invite.grant, &invite.capability, source, clock(0))
            .unwrap();
        (invite, guest)
    }

    fn read(
        &mut self,
        guest: &Guest,
        resume: Option<Cursor>,
        ms: u64,
    ) -> crate::sharing::Result<Update> {
        self.store.read(
            guest,
            &request(resume),
            Source {
                endpoint: &self.endpoint,
                cursor: self.cursor,
                transcript: &self.transcript,
            },
            clock(ms),
        )
    }

    fn push(&mut self, id: u64, kind: EntryKind) {
        self.transcript.entries.push(Entry {
            id: EntryId(id),
            kind,
        });
        self.cursor.seq += 1;
    }
}

fn request(cursor: Option<Cursor>) -> Vec<u8> {
    serde_json::to_vec(&Read {
        version: VERSION,
        cursor,
    })
    .unwrap()
}

fn agent(text: &str) -> EntryKind {
    EntryKind::Agent { text: text.into() }
}

fn snapshot(update: Update) -> (Cursor, Transcript) {
    match update {
        Update::Snapshot { cursor, transcript } => (cursor, *transcript),
        other => panic!("expected snapshot, got {other:?}"),
    }
}
