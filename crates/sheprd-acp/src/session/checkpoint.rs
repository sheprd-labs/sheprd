//! Private host-side history for a provider which resumes but does not replay.
//! Only display history is saved. Loading the actual agent session still has
//! to succeed before any of it is offered to the newly attached clients.
use super::{Session, Update};
use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{
    Agent, Change, Entry, EntryKind, MAX_FRAME_SIZE, PullRequest, Transcript,
};
use std::{
    fs::File,
    io::Read as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

// A full checkpoint shares the chat's frame bound. Prune by session so
// streaming a large tool call does not accumulate historical snapshots.
const MAX_FILES: usize = 64;
const INTERVAL: Duration = Duration::from_secs(2);

#[derive(Deserialize, Serialize)]
struct Saved {
    version: u8,
    #[serde(default)]
    identity: Option<String>,
    entries: Vec<Entry>,
    pull_requests: Vec<PullRequest>,
    truncated: bool,
}

impl Saved {
    fn read(path: &Path) -> Option<Self> {
        let mut bytes = Vec::new();
        File::open(path)
            .ok()?
            .take(MAX_FRAME_SIZE as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_FRAME_SIZE {
            return None;
        }
        let saved: Self = serde_json::from_slice(&bytes).ok()?;
        let name = path.file_name()?.to_str()?;
        // Early v1 checkpoints stored the identity only in their filename.
        (saved.version == 1
            && saved.identity.as_deref().is_none_or(|id| id == name)
            && super::keep_from(&saved.entries) == 0
            && saved.pull_requests.len() <= sheprd_chat_protocol::MAX_PULL_REQUESTS)
            .then_some(saved)
    }

    fn write(self, path: &Path) -> anyhow::Result<()> {
        let dir = path
            .parent()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        if !path.exists() {
            crate::briefs::prune(dir, path, MAX_FILES);
        }
        let bytes = serde_json::to_vec(&self)?;
        anyhow::ensure!(
            bytes.len() <= MAX_FRAME_SIZE,
            "conversation exceeds its snapshot limit"
        );
        sheprd_chat_protocol::restore::write_private(dir, path, &bytes)?;
        Ok(())
    }
}

type Pending = Arc<Mutex<Option<(PathBuf, Saved)>>>;

pub(super) struct Checkpoint {
    dir: PathBuf,
    path: Option<PathBuf>,
    dirty: bool,
    last: Option<Instant>,
    pending: Pending,
    wake: Option<mpsc::SyncSender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Checkpoint {
    fn new(dir: PathBuf) -> std::io::Result<Self> {
        let pending = Pending::default();
        let slot = pending.clone();
        let (wake, rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("acp-save-history".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    let item = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
                    if let Some((path, saved)) = item
                        && let Err(error) = saved.write(&path)
                    {
                        eprintln!("sheprd-acp: could not save conversation: {error:#}");
                    }
                }
            })?;
        Ok(Self {
            dir,
            path: None,
            dirty: false,
            last: None,
            pending,
            wake: Some(wake),
            worker: Some(worker),
        })
    }

    pub(super) fn changed(&mut self, change: &Change) {
        self.dirty |= matches!(
            change,
            Change::Push { .. }
                | Change::Append { .. }
                | Change::Replace { .. }
                | Change::Truncate { .. }
                | Change::PullRequest { .. }
        );
    }

    fn save(&mut self, transcript: &Transcript, force: bool) {
        let Some(path) = &self.path else { return };
        if !self.dirty || (!force && self.last.is_some_and(|last| last.elapsed() < INTERVAL)) {
            return;
        }
        let Some(identity) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        // The next load places its own divider after the recovered history.
        let divider = transcript.session.as_ref().and_then(|s| s.continued);
        let saved = Saved {
            version: 1,
            identity: Some(identity.to_owned()),
            entries: transcript
                .entries
                .iter()
                .filter(|entry| Some(entry.id) != divider)
                .cloned()
                .collect(),
            pull_requests: transcript.pull_requests.clone(),
            truncated: transcript.truncated,
        };
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((path.clone(), saved));
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
        self.last = Some(Instant::now());
        self.dirty = false;
    }
}

impl Drop for Checkpoint {
    fn drop(&mut self) {
        self.wake.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn key(agent: Agent, cwd: &Path, session: &str) -> Option<String> {
    if !sheprd_chat_protocol::restore::session_id_is_plain(session) {
        return None;
    }
    let directories: Vec<_> = sheprd_client::herdr::AgentKind::parse(agent.name())
        .into_iter()
        .flat_map(sheprd_client::herdr::sessions::environment_keys)
        .map(|key| (*key, std::env::var(key).ok()))
        .collect();
    let identity = serde_json::to_vec(&(agent, cwd, session, directories)).ok()?;
    Some(format!(
        "{}.json",
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, &identity)
    ))
}

impl<R: FnMut(Update)> Session<R> {
    pub(crate) fn keep_history(&mut self, dir: PathBuf) {
        match Checkpoint::new(dir) {
            Ok(checkpoint) => self.checkpoint = Some(checkpoint),
            Err(error) => eprintln!("sheprd-acp: could not start conversation writer: {error}"),
        }
    }

    pub(super) fn save_history(&mut self, force: bool) {
        if self.ready
            && let Some(checkpoint) = &mut self.checkpoint
        {
            checkpoint.save(&self.transcript, force || self.turn.is_none());
        }
    }

    pub(super) fn open_history(&mut self, session: &str, loaded: bool) {
        let Some(checkpoint) = &mut self.checkpoint else {
            return;
        };
        checkpoint.path = self
            .config
            .harness
            .and_then(|agent| key(agent, &self.config.cwd, session))
            .map(|file| checkpoint.dir.join(file));
        checkpoint.last = None;
        if !loaded {
            return;
        }
        let Some(saved) = checkpoint.path.as_deref().and_then(Saved::read) else {
            return;
        };
        if !self
            .transcript
            .entries
            .iter()
            .any(|e| matches!(e.kind, EntryKind::User { .. } | EntryKind::Agent { .. }))
        {
            self.transcript.truncated |= saved.truncated;
            for mut entry in saved.entries {
                entry.id = self.projector.next_id();
                let change = Change::Push { entry };
                self.emit(change.clone());
                self.find_brief(&change);
            }
            if !self.thread.started() {
                self.thread =
                    super::thread::Thread::new(self.config.harness, &self.transcript.entries);
            }
        }
        for pull_request in saved.pull_requests {
            self.emit(Change::PullRequest { pull_request });
        }
    }
}
