//! Draft read-only shared-session protocol. This is deliberately separate from
//! the local attach protocol: there is no guest command or host path to dispatch.
//! Admission and projection are the host's responsibility, before encoding.

use serde::{Deserialize, Serialize};

use crate::{Change, Cursor, Transcript};

pub const VERSION: u32 = 0;
pub const MAX_REQUEST_BYTES: usize = 1024;
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// Ask for the permitted transcript since a cursor, or a snapshot on first read.
/// A cursor is only a resume hint, never evidence of authorization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub version: u32,
    pub cursor: Option<Cursor>,
}

/// Cursors and entry IDs belong to the shared projection, never the source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", deny_unknown_fields)]
pub enum Update {
    Changes {
        cursor: Cursor,
        changes: Vec<Change>,
    },
    Snapshot {
        cursor: Cursor,
        transcript: Box<Transcript>,
    },
    Unchanged {
        cursor: Cursor,
    },
}

pub mod admission;
