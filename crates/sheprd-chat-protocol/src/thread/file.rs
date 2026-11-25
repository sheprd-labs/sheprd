//! A thread's transcript file: every entry of the conversation with its id,
//! across agents, apart from the window attached clients see, which stays
//! trimmed as before.
//!
//! The file is JSON lines, appended to. An entry is written again whenever
//! it changed since it was last written, and the last line for an id wins.
//! A line cut short by a killed program, or one this version cannot read,
//! is skipped.
//!
//! Past [`MAX_FILE`] the file is rewritten without its oldest entries, down
//! to [`TRIM_TO`]. The thread's first prompt is pinned and never trimmed,
//! and each removed run of ids is recorded, so a read of one answers
//! "trimmed" rather than nothing. An entry too large to keep is recorded the
//! same way.

use std::{
    collections::BTreeMap,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{Error, ThreadId};
use crate::{Entry, EntryId, EntryKind};

/// The most bytes a transcript file holds.
pub const MAX_FILE: u64 = 64 * 1024 * 1024;
/// What a trimmed file is cut down to, so trimming stays rare.
pub const TRIM_TO: u64 = MAX_FILE / 2;
/// The longest line an entry may take; a larger entry is recorded as
/// trimmed instead.
pub const MAX_LINE: usize = 4 * 1024 * 1024;

/// One line of the file.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "line")]
enum Line {
    Entry { entry: Entry },
    Trimmed(Trimmed),
}

/// Entries `from` through `to` that the file no longer holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Trimmed {
    pub from: EntryId,
    pub to: EntryId,
}

impl Trimmed {
    pub fn contains(&self, id: EntryId) -> bool {
        (self.from..=self.to).contains(&id)
    }
}

/// What a transcript file holds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Contents {
    /// Every entry kept, each at its latest, in id order.
    pub entries: Vec<Entry>,
    /// The ranges no longer kept, in id order, none overlapping an entry.
    pub trimmed: Vec<Trimmed>,
}

impl Contents {
    /// Reads the file at `path`; a file that does not exist holds nothing.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = match crate::private::read_up_to(path, MAX_FILE) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        if bytes.len() as u64 > MAX_FILE {
            return Err(Error::TranscriptSize);
        }
        Ok(Self::parse(&bytes))
    }

    fn parse(bytes: &[u8]) -> Self {
        let mut entries = BTreeMap::new();
        let mut trimmed = Vec::new();
        for line in bytes.split(|byte| *byte == b'\n') {
            if line.is_empty() || line.len() > MAX_LINE {
                continue;
            }
            match serde_json::from_slice::<Line>(line) {
                Ok(Line::Entry { entry }) => {
                    entries.insert(entry.id, entry);
                }
                Ok(Line::Trimmed(range)) if range.from <= range.to => trimmed.push(range),
                Ok(Line::Trimmed(_)) | Err(_) => {}
            }
        }
        // What a range says is gone stays gone, even when a later line wrote
        // it again: a tool call that changed after it was trimmed.
        entries.retain(|id, _| !trimmed.iter().any(|range| range.contains(*id)));
        let entries: Vec<Entry> = entries.into_values().collect();
        let trimmed = merge(trimmed, &entries);
        Self { entries, trimmed }
    }

    /// The newest id the file knows of, kept or trimmed.
    pub fn last_id(&self) -> Option<EntryId> {
        let entry = self.entries.last().map(|entry| entry.id);
        let range = self.trimmed.iter().map(|range| range.to).max();
        entry.max(range)
    }

    /// Whether `id` was in the file once and is not any more.
    pub fn is_trimmed(&self, id: EntryId) -> bool {
        self.trimmed.iter().any(|range| range.contains(id))
    }

    /// The thread's first prompt, which trimming never removes.
    fn pinned(&self) -> Option<EntryId> {
        self.entries
            .iter()
            .find(|entry| matches!(entry.kind, EntryKind::User { .. }))
            .map(|entry| entry.id)
    }

    /// Takes in `entries`, each replacing the one with its id.
    fn update(&mut self, entries: &[&Entry]) {
        for entry in entries {
            if self.is_trimmed(entry.id) {
                continue;
            }
            match self.entries.binary_search_by_key(&entry.id, |kept| kept.id) {
                Ok(at) => self.entries[at] = (*entry).clone(),
                Err(at) => self.entries.insert(at, (*entry).clone()),
            }
        }
    }

    /// Drops the oldest entries but the pinned one until the lines left
    /// weigh at most `budget`, recording what went.
    fn trim(&mut self, budget: u64) -> Result<(), Error> {
        let pinned = self.pinned();
        let mut lines: Vec<(EntryId, u64)> = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            lines.push((entry.id, entry_line(entry)?.len() as u64));
        }
        let mut total: u64 = lines.iter().map(|(_, size)| size).sum();
        let mut gone = Vec::new();
        for (id, size) in lines {
            if total <= budget {
                break;
            }
            if Some(id) == pinned {
                continue;
            }
            total -= size;
            gone.push(id);
        }
        if gone.is_empty() {
            return Ok(());
        }
        self.entries
            .retain(|entry| gone.binary_search(&entry.id).is_err());
        let mut trimmed = std::mem::take(&mut self.trimmed);
        trimmed.extend(gone.into_iter().map(|id| Trimmed { from: id, to: id }));
        self.trimmed = merge(trimmed, &self.entries);
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        for range in &self.trimmed {
            serde_json::to_writer(&mut bytes, &Line::Trimmed(*range))?;
            bytes.push(b'\n');
        }
        for entry in &self.entries {
            bytes.extend(entry_line(entry)?);
        }
        Ok(bytes)
    }
}

/// `ranges` sorted, with any two that no kept entry separates made one.
fn merge(mut ranges: Vec<Trimmed>, entries: &[Entry]) -> Vec<Trimmed> {
    ranges.sort_by_key(|range| range.from);
    let mut merged: Vec<Trimmed> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            let between = entries
                .iter()
                .any(|entry| entry.id > last.to && entry.id < range.from);
            if !between {
                last.to = last.to.max(range.to);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

/// `entry` as one line with its line break, or, when it is larger than
/// [`MAX_LINE`], the line that records it as trimmed.
fn entry_line(entry: &Entry) -> Result<Vec<u8>, Error> {
    let mut bytes = serde_json::to_vec(&Line::Entry {
        entry: entry.clone(),
    })?;
    if bytes.len() >= MAX_LINE {
        bytes = serde_json::to_vec(&Line::Trimmed(Trimmed {
            from: entry.id,
            to: entry.id,
        }))?;
    }
    bytes.push(b'\n');
    Ok(bytes)
}

/// Appends to one thread's transcript file.
#[derive(Debug)]
pub struct Writer {
    dir: PathBuf,
    path: PathBuf,
    /// The file's size as this writer left it.
    size: u64,
    /// The file may end inside a line, as a killed program or a failed
    /// write leaves it: the next append starts on a line of its own.
    torn: bool,
}

impl Writer {
    /// The writer of thread `id`'s file in `dir`, which may not exist yet.
    pub fn open(dir: &Path, id: &ThreadId) -> Result<Self, Error> {
        let path = super::transcript_path(dir, id);
        let (size, torn) = match std::fs::File::open(&path) {
            Ok(mut file) => {
                let size = file.metadata()?.len();
                (size, size > 0 && !ends_a_line(&mut file)?)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (0, false),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            dir: dir.to_owned(),
            path,
            size,
            torn,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `entries` at their latest, trimming the file first when they
    /// would take it past [`MAX_FILE`].
    pub fn append(&mut self, entries: &[&Entry]) -> Result<(), Error> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut bytes = Vec::new();
        for entry in entries {
            bytes.extend(entry_line(entry)?);
        }
        if self.torn {
            bytes.insert(0, b'\n');
        }
        if self.size + bytes.len() as u64 > MAX_FILE {
            return self.rewrite(entries);
        }
        crate::private::create_dir(&self.dir)?;
        let written =
            crate::private::open_append(&self.path).and_then(|mut file| file.write_all(&bytes));
        // A write that failed part way may have left half a line.
        self.torn = written.is_err();
        written?;
        self.size += bytes.len() as u64;
        Ok(())
    }

    /// Rewrites the file with `entries` taken in and its oldest entries
    /// trimmed to [`TRIM_TO`], in one rename.
    fn rewrite(&mut self, entries: &[&Entry]) -> Result<(), Error> {
        // A file that does not read is never replaced by less.
        let mut contents = Contents::read(&self.path)?;
        contents.update(entries);
        contents.trim(TRIM_TO)?;
        let bytes = contents.encode()?;
        crate::private::write(&self.dir, &self.path, &bytes)?;
        self.size = bytes.len() as u64;
        self.torn = false;
        Ok(())
    }

    /// Trims the file down to `budget` bytes now, for tests of trimming
    /// without writing [`MAX_FILE`] bytes.
    #[cfg(test)]
    pub(crate) fn trim_to(&mut self, budget: u64) -> Result<(), Error> {
        let mut contents = Contents::read(&self.path)?;
        contents.trim(budget)?;
        let bytes = contents.encode()?;
        crate::private::write(&self.dir, &self.path, &bytes)?;
        self.size = bytes.len() as u64;
        self.torn = false;
        Ok(())
    }
}

/// Whether `file`'s last byte ends a line.
fn ends_a_line(file: &mut std::fs::File) -> io::Result<bool> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0];
    file.read_exact(&mut last)?;
    Ok(last[0] == b'\n')
}
