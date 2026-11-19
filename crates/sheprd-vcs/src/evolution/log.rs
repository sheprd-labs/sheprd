//! The history file kept in a branch's history ref: the branch's own commits
//! as last seen, and the markers saying which commit replaced which.
//!
//! One record per line, fields separated by single spaces, the last field
//! free text; written and read only by this module. Text from commits and
//! the daemon is cleaned to one bounded line before it is stored.

use super::Error;
use crate::Result;

const HEADER: &str = "herdr-evolution 1";
/// Markers kept per branch, newest last; older ones are forgotten, and so is
/// the reason Git keeps the commits they name.
pub const MARKER_LIMIT: usize = 500;
/// Longest subject or credit kept.
const TEXT_LIMIT: usize = 200;
/// Longest `Change-Id` trailer value kept; longer ones are not matched on.
const CHANGE_ID_LIMIT: usize = 128;

/// One commit of the branch's own, oldest first in a stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    /// `git patch-id --stable`: the same change rebased keeps it. Merges and
    /// empty commits have none.
    pub patch_id: Option<String>,
    /// The commit's `Change-Id` trailer, when it has one.
    pub change_id: Option<String>,
    pub subject: String,
}

/// How a replacement was recognized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// Both carry the same `Change-Id` trailer.
    ChangeId,
    /// The same patch on another base: a rebase or a reword.
    Patch,
    /// The same subject at the same place in the stack: an amend.
    Subject,
}

/// What became of a commit that left the branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Replaced {
        new: String,
        how: Match,
    },
    /// Nothing on the branch was recognized as its successor: dropped,
    /// squashed into another, or changed past recognizing.
    Dropped,
}

/// A commit that left the branch, and when and in whose turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    /// Unix seconds.
    pub time: i64,
    pub old: String,
    pub outcome: Outcome,
    /// The agents whose turn ended when it was seen; none when it was seen
    /// as a turn started, so made outside one.
    pub by: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub stack: Vec<Commit>,
    pub markers: Vec<Marker>,
}

impl Log {
    pub fn render(&self) -> String {
        let mut text = format!("{HEADER}\n");
        for commit in &self.stack {
            text.push_str(&format!(
                "commit {} {} {} {}\n",
                commit.sha,
                commit.patch_id.as_deref().unwrap_or("-"),
                commit.change_id.as_deref().unwrap_or("-"),
                clean(&commit.subject),
            ));
        }
        for marker in &self.markers {
            let (new, how) = match &marker.outcome {
                Outcome::Replaced { new, how } => (new.as_str(), how.token()),
                Outcome::Dropped => ("-", "dropped"),
            };
            text.push_str(&format!(
                "marker {} {} {new} {how} {}\n",
                marker.time,
                marker.old,
                marker.by.as_deref().map(clean).unwrap_or_default(),
            ));
        }
        text
    }

    /// An empty text is an empty log: the branch has no history ref yet.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let mut log = Self::default();
        match lines.next() {
            None => return Ok(log),
            Some(HEADER) => {}
            Some(_) => return Err(Error::Log.into()),
        }
        for line in lines {
            match line.split_once(' ') {
                Some(("commit", rest)) => log.stack.push(parse_commit(rest)?),
                Some(("marker", rest)) => log.markers.push(parse_marker(rest)?),
                _ => return Err(Error::Log.into()),
            }
        }
        Ok(log)
    }

    /// Add `markers` and keep only the newest [`MARKER_LIMIT`].
    pub fn record(&mut self, markers: impl IntoIterator<Item = Marker>) {
        self.markers.extend(markers);
        let excess = self.markers.len().saturating_sub(MARKER_LIMIT);
        self.markers.drain(..excess);
    }

    /// Every commit a marker names as replaced or dropped, once each: the
    /// history ref's parents, so Git keeps them.
    pub fn kept(&self) -> Vec<&str> {
        let mut kept: Vec<&str> = Vec::new();
        for marker in &self.markers {
            if !kept.contains(&marker.old.as_str()) {
                kept.push(&marker.old);
            }
        }
        kept
    }
}

impl Match {
    fn token(self) -> &'static str {
        match self {
            Self::ChangeId => "change-id",
            Self::Patch => "patch",
            Self::Subject => "subject",
        }
    }
}

fn parse_commit(rest: &str) -> Result<Commit> {
    let mut fields = rest.splitn(4, ' ');
    let (Some(sha), Some(patch_id), Some(change_id), Some(subject)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return Err(Error::Log.into());
    };
    let patch_id = optional(patch_id);
    if !is_sha(sha) || patch_id.is_some_and(|id| !is_sha(id)) {
        return Err(Error::Log.into());
    }
    Ok(Commit {
        sha: sha.to_owned(),
        patch_id: patch_id.map(str::to_owned),
        change_id: optional(change_id).and_then(change_id_of),
        subject: subject.to_owned(),
    })
}

fn parse_marker(rest: &str) -> Result<Marker> {
    let mut fields = rest.splitn(5, ' ');
    let (Some(time), Some(old), Some(new), Some(how), Some(by)) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    ) else {
        return Err(Error::Log.into());
    };
    let time = time.parse().map_err(|_| Error::Log)?;
    let outcome = match (how, optional(new)) {
        ("dropped", None) => Outcome::Dropped,
        (how, Some(new)) if is_sha(new) => Outcome::Replaced {
            new: new.to_owned(),
            how: match how {
                "change-id" => Match::ChangeId,
                "patch" => Match::Patch,
                "subject" => Match::Subject,
                _ => return Err(Error::Log.into()),
            },
        },
        _ => return Err(Error::Log.into()),
    };
    if !is_sha(old) {
        return Err(Error::Log.into());
    }
    Ok(Marker {
        time,
        old: old.to_owned(),
        outcome,
        by: Some(by).filter(|by| !by.is_empty()).map(str::to_owned),
    })
}

fn optional(field: &str) -> Option<&str> {
    Some(field).filter(|field| *field != "-")
}

/// A full object name: SHA-1 or SHA-256, lowercase hex.
pub fn is_sha(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A Gerrit-style `Change-Id` value from 20 random bytes: `I` and 40 hex
/// digits, which Gerrit, `git-branchless`, and this matcher all accept.
pub fn new_change_id(random: [u8; 20]) -> String {
    let mut id = String::with_capacity(41);
    id.push('I');
    for byte in random {
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

/// A trailer value usable as a key: one printable word, bounded.
pub fn change_id_of(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value != "-"
        && value.len() <= CHANGE_ID_LIMIT
        && value.chars().all(|c| c.is_ascii_graphic()))
    .then(|| value.to_owned())
}

/// One line, no control characters, bounded: subjects and agent names are
/// untrusted text.
pub fn clean(text: &str) -> String {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(TEXT_LIMIT)
        .collect();
    text.trim().to_owned()
}
