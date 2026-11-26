//! A thread: one conversation that keeps its id and its transcript while
//! the agent holding it changes underneath (`docs/threads-plan.md`).
//!
//! A thread is a list of **segments**, one per agent session that held the
//! conversation, oldest first. The chat program keeps a [`Record`] of it in
//! [`threads_dir`] once a prompt reached an agent, and appends every entry
//! to the thread's own transcript file ([`file`]) beside it. Both outlive
//! the program: a thread is kept until it is deleted, not only when its chat
//! was killed, as a restore record is.

mod error;
pub mod file;
pub mod read;

pub use error::Error;

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Agent, EntryId};

/// The record format this program writes and reads.
pub const VERSION: u32 = 1;
/// The most segments a thread keeps; past it the oldest two fold into one.
pub const MAX_SEGMENTS: usize = 64;
/// The longest model name a segment keeps.
pub const MAX_MODEL: usize = 200;
/// The longest title a record keeps, in bytes.
pub const MAX_TITLE: usize = 200;
/// The most bytes a record may hold: 64 full segments fit.
pub const MAX_RECORD_SIZE: u64 = 64 * 1024;

const RECORD_EXTENSION: &str = "json";
const TRANSCRIPT_EXTENSION: &str = "jsonl";

/// A thread's identity, kept across agents, restarts, and restores.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(Uuid);

impl ThreadId {
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(formatter)
    }
}

impl FromStr for ThreadId {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        Uuid::parse_str(text).map(Self).map_err(|_| Error::Id)
    }
}

/// One agent session that held the conversation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    /// The last entry before this agent took over; `None` when it held the
    /// conversation from its start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<EntryId>,
    pub agent: Agent,
    /// The model it last ran, as its setting names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Its own session id, once the session opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Segment {
    pub fn new(agent: Agent, after: Option<EntryId>) -> Self {
        Self {
            after,
            agent,
            model: None,
            session: None,
        }
    }
}

/// A thread as clients see it: its id, and where each agent began.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    /// Oldest first; never empty in a thread a program sent.
    pub segments: Vec<Segment>,
}

impl Thread {
    /// A thread `agent` starts.
    pub fn new(id: ThreadId, agent: Agent) -> Self {
        Self {
            id,
            segments: vec![Segment::new(agent, None)],
        }
    }

    /// The segment holding the conversation now.
    pub fn current(&self) -> Option<&Segment> {
        self.segments.last()
    }

    pub fn current_mut(&mut self) -> Option<&mut Segment> {
        self.segments.last_mut()
    }

    /// `agent` takes over after entry `after`. Past [`MAX_SEGMENTS`], the
    /// oldest two segments fold into one: the second, from where the first
    /// began.
    pub fn switch(&mut self, agent: Agent, after: Option<EntryId>) {
        self.segments.push(Segment::new(agent, after));
        while self.segments.len() > MAX_SEGMENTS {
            let oldest = self.segments.remove(0);
            if let Some(next) = self.segments.first_mut() {
                next.after = oldest.after;
            }
        }
    }

    /// Records the session the current segment opened.
    pub fn opened(&mut self, session: &str) {
        if let Some(current) = self.current_mut() {
            current.session =
                crate::restore::session_id_is_plain(session).then(|| session.to_owned());
        }
    }

    /// Records the model the current segment runs now; `true` when it
    /// changed.
    pub fn ran(&mut self, model: &str) -> bool {
        let model = cut(model, MAX_MODEL);
        match self.current_mut() {
            Some(current) if current.model.as_deref() != Some(model) => {
                current.model = Some(model.to_owned());
                true
            }
            _ => false,
        }
    }

    /// Checks what a thread from elsewhere holds: at least one and at most
    /// [`MAX_SEGMENTS`] segments, plain session ids, and short model names.
    pub fn validate(&self) -> Result<(), Error> {
        if self.segments.is_empty() || self.segments.len() > MAX_SEGMENTS {
            return Err(Error::Segments);
        }
        for segment in &self.segments {
            if segment
                .session
                .as_deref()
                .is_some_and(|session| !crate::restore::session_id_is_plain(session))
            {
                return Err(Error::Session);
            }
            if segment
                .model
                .as_deref()
                .is_some_and(|model| model.len() > MAX_MODEL)
            {
                return Err(Error::Model);
            }
        }
        Ok(())
    }
}

/// What the chat program keeps of a thread beside its transcript file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    /// When the thread's first prompt was sent (Unix milliseconds).
    pub created_ms: u64,
    /// The folder the thread's agents work in, canonical.
    pub cwd: PathBuf,
    pub thread: Thread,
    /// The first line of the thread's first prompt, cut to [`MAX_TITLE`].
    pub title: String,
    /// When the record was last written (Unix milliseconds).
    pub updated_ms: u64,
}

/// The record as it is stored; [`Record`] is what a valid one reads as.
#[derive(Deserialize, Serialize)]
struct Stored {
    created_ms: u64,
    cwd: PathBuf,
    thread: Thread,
    title: String,
    updated_ms: u64,
    version: u32,
}

impl From<&Record> for Stored {
    fn from(record: &Record) -> Self {
        Self {
            created_ms: record.created_ms,
            cwd: record.cwd.clone(),
            thread: record.thread.clone(),
            title: record.title.clone(),
            updated_ms: record.updated_ms,
            version: VERSION,
        }
    }
}

impl TryFrom<Stored> for Record {
    type Error = Error;

    fn try_from(stored: Stored) -> Result<Self, Error> {
        if stored.version != VERSION {
            return Err(Error::Version(stored.version));
        }
        let record = Self {
            created_ms: stored.created_ms,
            cwd: stored.cwd,
            thread: stored.thread,
            title: stored.title,
            updated_ms: stored.updated_ms,
        };
        record.validate()?;
        Ok(record)
    }
}

impl Record {
    pub fn validate(&self) -> Result<(), Error> {
        self.thread.validate()?;
        if !self.cwd.is_absolute() {
            return Err(Error::Path);
        }
        if self.title.len() > MAX_TITLE {
            return Err(Error::Title);
        }
        Ok(())
    }

    /// Writes the record into `dir`, replacing the thread's earlier one in
    /// a single rename, readable only by this user.
    pub fn write(&self, dir: &Path) -> Result<(), Error> {
        self.validate()?;
        let bytes = serde_json::to_vec(&Stored::from(self))?;
        if bytes.len() as u64 > MAX_RECORD_SIZE {
            return Err(Error::Size);
        }
        Ok(crate::private::write(
            dir,
            &record_path(dir, &self.thread.id),
            &bytes,
        )?)
    }

    /// Reads the record at `path`, at most [`MAX_RECORD_SIZE`] bytes.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = crate::private::read_up_to(path, MAX_RECORD_SIZE)?;
        if bytes.len() as u64 > MAX_RECORD_SIZE {
            return Err(Error::Size);
        }
        Record::try_from(serde_json::from_slice::<Stored>(&bytes)?)
    }
}

/// Exclusive ownership of a thread's record and transcript. The separate
/// lock file is never renamed or removed, so atomic rewrites cannot release
/// ownership or let another process lock a different inode.
#[derive(Debug)]
pub struct Lock {
    _file: std::fs::File,
}

impl Lock {
    pub fn acquire(dir: &Path, id: &ThreadId) -> Result<Self, Error> {
        crate::private::create_dir(dir)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options.open(dir.join(format!("{id}.lock")))?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => Error::Locked,
            std::fs::TryLockError::Error(error) => Error::Io(error),
        })?;
        Ok(Self { _file: file })
    }
}

/// A thread's title from its first prompt: the first line with text, cut
/// to [`MAX_TITLE`] bytes on a character boundary.
pub fn title(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    cut(line, MAX_TITLE).to_owned()
}

fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The private directory thread records and transcript files live in:
/// `threads` beside the chat sockets of [`crate::endpoint_dir`], apart from
/// restore records, which come and go with their panes.
pub fn threads_dir() -> Option<PathBuf> {
    threads_dir_with(|name| std::env::var_os(name))
}

/// [`threads_dir`] with the environment supplied, for tests.
pub fn threads_dir_with(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    Some(crate::restore::restore_dir_with(var)?.with_file_name("threads"))
}

pub fn record_path(dir: &Path, id: &ThreadId) -> PathBuf {
    dir.join(format!("{id}.{RECORD_EXTENSION}"))
}

/// Where the thread's transcript file lives ([`file`]).
pub fn transcript_path(dir: &Path, id: &ThreadId) -> PathBuf {
    dir.join(format!("{id}.{TRANSCRIPT_EXTENSION}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
