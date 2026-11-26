//! Single-writer, bounded, private state. Every successful mutation is durable.
//! A failed write disables the live store, including previously admitted readers.

use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sheprd_client::private_file;

use crate::sharing::{
    Error, Result,
    model::{MAX_GRANTS, MAX_LIFETIME, MAX_SHARES, Share},
};

const VERSION: u32 = 1;
const MAX_STATE_BYTES: usize = 256 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    version: u32,
    pub(crate) shares: Vec<Share>,
}

#[derive(Debug)]
pub(crate) struct Storage {
    path: PathBuf,
    _lock: File,
}

impl Storage {
    pub(crate) fn open(directory: &Path) -> Result<(Self, State)> {
        let lock_path = directory.join("lock");
        private_file::prepare(&lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| file_error(&lock_path, e))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(Error::Busy),
            Err(TryLockError::Error(e)) => return Err(file_error(&lock_path, e)),
        }
        let path = directory.join("shares.json");
        private_file::prepare(&path)?;
        let mut bytes = Vec::new();
        File::open(&path)
            .and_then(|f| f.take(MAX_STATE_BYTES as u64 + 1).read_to_end(&mut bytes))
            .map_err(|e| file_error(&path, e))?;
        if bytes.len() > MAX_STATE_BYTES {
            return Err(Error::Limit);
        }
        let state = if bytes.is_empty() {
            State {
                version: VERSION,
                shares: Vec::new(),
            }
        } else {
            let state: State = serde_json::from_slice(&bytes)?;
            state.validate()?;
            state
        };
        Ok((Self { path, _lock: lock }, state))
    }

    pub(crate) fn save(&self, state: &State) -> Result<()> {
        state.validate()?;
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() > MAX_STATE_BYTES {
            return Err(Error::Limit);
        }
        let directory = self.path.parent().ok_or(Error::InvalidState)?;
        private_file::prepare(&self.path)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(directory).map_err(|e| file_error(&self.path, e))?;
        temp.write_all(&bytes)
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|e| file_error(&self.path, e))?;
        temp.persist(&self.path)
            .map_err(|e| file_error(&self.path, e.error))?;
        #[cfg(unix)]
        File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| file_error(directory, e))?;
        Ok(())
    }
}

impl State {
    fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(Error::Version);
        }
        if self.shares.len() > MAX_SHARES {
            return Err(Error::Limit);
        }
        let mut ids = std::collections::BTreeSet::new();
        for share in &self.shares {
            if share.id.0.is_nil()
                || !ids.insert(share.id)
                || !share.binding.valid()
                || share.expires_ms <= share.created_ms
                || share.expires_ms - share.created_ms > MAX_LIFETIME.as_millis() as u64
                || share.grants.len() > MAX_GRANTS
            {
                return Err(Error::InvalidState);
            }
            let mut grants = std::collections::BTreeSet::new();
            for grant in &share.grants {
                if grant.id.0.is_nil()
                    || !grants.insert(grant.id)
                    || grant.expires_ms > share.expires_ms
                    || grant.expires_ms <= share.created_ms
                {
                    return Err(Error::InvalidState);
                }
            }
        }
        Ok(())
    }
}

fn file_error(path: &Path, source: std::io::Error) -> Error {
    Error::File {
        path: path.to_owned(),
        source,
    }
}
