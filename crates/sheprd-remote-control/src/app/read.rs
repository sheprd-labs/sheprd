//! This computer's read marks on disk (`control::read`), and
//! `sheprd-remote-control read`, which Sheprd runs over SSH on another
//! computer. Only a running daemon knows its chats' replies, so without one
//! nothing is unread there and the command prints nothing.

use crate::control::{
    self, Request, Response,
    read::{Change, Marks, Unread},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILE: &str = "read.json";

/// Where the marks of the daemon whose state is `state_dir` are kept.
pub fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

/// The marks saved in `file`, bounded. Without them, marks start at
/// `now_ms`, so the replies already there count as read: a damaged file
/// costs only what was unread. Blocking.
pub fn load(file: &Path, now_ms: u64) -> Marks {
    fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Marks>(&bytes).ok())
        .map_or_else(|| Marks::starting(now_ms), Marks::bounded)
}

/// Replaces `file` with `marks`. Blocking.
pub fn save(file: &Path, marks: &Marks) -> super::Result<()> {
    super::edited::save(file, marks)
}

/// Makes `change`, if any, through the daemon of `state_dir` and returns
/// what is unread there; `None` when no daemon runs. Blocking.
pub fn exchange(state_dir: &Path, change: Option<Change>) -> super::Result<Option<Unread>> {
    let address = control::address(state_dir);
    match control::request(&address, &Request::Read { change }) {
        Ok(Response::Read(unread)) => Ok(Some(unread)),
        Ok(_) => Err(super::Error::Unexpected),
        Err(control::Error::Connect(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
