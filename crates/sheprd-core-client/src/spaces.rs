//! A core's shared workspaces and tabs (`sheprd_session::spaces`) in its
//! `sheprd.spaces` document: read, and changed by edits written back over
//! the version they were made to. A write another client got in first is
//! refused; the edit is then made again to what that client wrote, so both
//! changes stay.
//!
//! Blocking, like every [`Core`] call: run these on a worker thread.

use crate::{Core, Error, Method, Result, wire::Doc};
use serde_json::Value;
use sheprd_session::spaces::{DOC, Edit, Spaces};

/// How many times an edit is made again after another client's write
/// came first. Each refusal means another writer made progress.
const ATTEMPTS: usize = 8;

/// The core's shared workspaces and tabs, and the document version they
/// were read at: zero for a document never written.
pub fn read(core: &Core) -> Result<(Spaces, u64)> {
    parse(core.doc_get(DOC)?)
}

/// Makes `edits` to the core's shared workspaces and answers what the core
/// then holds, with its version. An edit that fails leaves the document
/// as it was.
pub fn update(core: &Core, edits: &[Edit]) -> Result<(Spaces, u64)> {
    update_with(
        || core.doc_get(DOC),
        |value, version| core.doc_set(DOC, value, version),
        |spaces| {
            for edit in edits {
                spaces.apply(edit.clone())?;
            }
            Ok(())
        },
    )
}

/// [`update`] with any change: `change` sees the document as last read,
/// and may run more than once.
pub fn change(core: &Core, change: impl FnMut(&mut Spaces) -> Result<()>) -> Result<(Spaces, u64)> {
    update_with(
        || core.doc_get(DOC),
        |value, version| core.doc_set(DOC, value, version),
        change,
    )
}

/// [`change`] over any document store: `get` reads it and `set` writes over
/// a version, refusing a stale one as `doc.set` does.
pub(crate) fn update_with(
    mut get: impl FnMut() -> Result<Doc>,
    mut set: impl FnMut(&Value, u64) -> Result<u64>,
    mut change: impl FnMut(&mut Spaces) -> Result<()>,
) -> Result<(Spaces, u64)> {
    let mut attempt = 0;
    loop {
        let (mut spaces, version) = parse(get()?)?;
        let before = spaces.clone();
        change(&mut spaces)?;
        if spaces == before {
            return Ok((spaces, version));
        }
        match set(&spaces.to_value()?, version) {
            Ok(version) => return Ok((spaces, version)),
            Err(error) if conflict(&error) && attempt + 1 < ATTEMPTS => attempt += 1,
            Err(error) => return Err(error),
        }
    }
}

fn parse(doc: Doc) -> Result<(Spaces, u64)> {
    Ok((Spaces::from_value(doc.value)?, doc.version))
}

/// Whether `doc.set` was refused for a stale version.
fn conflict(error: &Error) -> bool {
    matches!(
        error,
        Error::Refused { method: Method::DocSet, kind: Some(kind), .. } if kind == "conflict"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
