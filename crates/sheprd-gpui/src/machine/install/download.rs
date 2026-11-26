//! Downloading Apple's macOS restore image: tens of gigabytes, so it goes to
//! a `.part` file first and a later try picks up where the last one stopped.
//! It runs on a thread of its own, as one request reads for many minutes.

use super::Error;
use sha2::{Digest as _, Sha256};
use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read as _, Write as _},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

/// How much is read before progress and the cancel flag are checked again.
const CHUNK: usize = 1 << 20;

/// How long a read waits for the next bytes. A stalled connection then ends
/// the try, keeping its part, rather than leave Stop waiting on it.
pub(super) const STALL: Duration = Duration::from_secs(20);

/// Room kept free beyond the download for what the install then writes:
/// macOS takes about this much on the new machine's disk.
pub(super) const INSTALL_ROOM: u64 = 30_000_000_000;

/// Room kept free beyond a Linux cloud image download for the sparse raw disk
/// conversion, EFI variable store, and NoCloud seed ISO.
pub(super) const LINUX_INSTALL_ROOM: u64 = 4_000_000_000;

/// What a download reports while it runs, read by the UI thread.
#[derive(Debug, Default)]
pub(crate) struct Counter {
    pub(crate) done: AtomicU64,
    /// Zero until the server says.
    pub(crate) total: AtomicU64,
    pub(crate) cancel: AtomicBool,
}

/// The agent Apple's server is reached with: HTTPS only, and no deadline on
/// the whole body, which takes as long as the line needs, only on each read.
pub(super) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_per_read(Some(STALL))
        .http_status_as_error(false)
        .timeout_resolve(Some(Duration::from_secs(15)))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_send_request(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .build()
        .into()
}

/// The file a download of `target` grows in until it is whole.
pub(super) fn partial(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_owned();
    name.push(".part");
    target.with_file_name(name)
}

/// Downloads `url` to `target`, resuming a `.part` file an earlier try left.
/// Nothing is at `target` until every byte the server named has arrived. It
/// refuses to start without room for the rest and `room` more, rather than
/// fill the disk at the end of a long download.
pub(super) fn fetch(
    agent: &ureq::Agent,
    url: &str,
    target: &Path,
    room: u64,
    counter: &Counter,
) -> Result<(), Error> {
    if let Ok(whole) = fs::metadata(target) {
        // Kept from a try whose install failed: the install still needs room.
        check_room(target, 0, room)?;
        counter.total.store(whole.len(), Ordering::Relaxed);
        counter.done.store(whole.len(), Ordering::Relaxed);
        return Ok(());
    }
    let part = partial(target);
    let mut offset = fs::metadata(&part).map_or(0, |meta| meta.len());
    let mut request = agent.get(url).header("Accept-Encoding", "identity");
    if offset > 0 {
        request = request.header("Range", format!("bytes={offset}-")); // i18n: data
    }
    let response = request
        .call()
        .map_err(|source| Error::Download(Box::new(source)))?;
    let status = response.status().as_u16();
    let length = response.body().content_length();
    // Before any file is made, so a refusal leaves none behind; the folder
    // must exist for its volume to be measured.
    if let Some(folder) = target.parent() {
        fs::create_dir_all(folder).map_err(|source| file_error(folder, source))?;
    }
    if let (200 | 206, Some(length)) = (status, length) {
        // A whole answer replaces the part, freeing what it took.
        let freed = if status == 200 { offset } else { 0 };
        check_room(target, length.saturating_sub(freed), room)?;
    }
    let mut file = match status {
        // The rest of what an earlier try began.
        206 => open(&part, true)?,
        // The whole file: the server ignored the range, or none was asked.
        200 => {
            offset = 0;
            open(&part, false)?
        }
        // Asked from past its end: the part is no good, and the next try
        // starts over.
        416 => {
            let _ = fs::remove_file(&part);
            return Err(Error::Status(status));
        }
        _ => return Err(Error::Status(status)),
    };
    let expected = length.map(|length| offset + length);
    counter
        .total
        .store(expected.unwrap_or(0), Ordering::Relaxed);
    counter.done.store(offset, Ordering::Relaxed);
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0; CHUNK];
    let mut got = offset;
    loop {
        if counter.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(source) if source.kind() == ErrorKind::Interrupted => continue,
            // Stop pressed while the line was stalled.
            Err(_) if counter.cancel.load(Ordering::Relaxed) => return Err(Error::Cancelled),
            Err(source) => return Err(Error::Download(Box::new(ureq::Error::Io(source)))),
        };
        file.write_all(&buffer[..read])
            .map_err(|source| file_error(&part, source))?;
        got += read as u64;
        counter.done.store(got, Ordering::Relaxed);
    }
    file.sync_all()
        .map_err(|source| file_error(&part, source))?;
    if let Some(expected) = expected.filter(|expected| got != *expected) {
        return Err(Error::Incomplete { got, expected });
    }
    fs::rename(&part, target).map_err(|source| file_error(target, source))
}

/// Refuses unless the volume holding `target` has `growth` more bytes free,
/// and `room` beyond them.
fn check_room(target: &Path, growth: u64, room: u64) -> Result<(), Error> {
    let Some(free) = target.parent().and_then(free_bytes) else {
        return Ok(());
    };
    let needed = growth.saturating_add(room);
    if free < needed {
        return Err(Error::NoRoom { needed, free });
    }
    Ok(())
}

/// What the volume holding `folder` has free.
fn free_bytes(folder: &Path) -> Option<u64> {
    let stat = nix::sys::statvfs::statvfs(folder).ok()?;
    u64::from(stat.blocks_available()).checked_mul(stat.fragment_size())
}

fn open(path: &Path, append: bool) -> Result<File, Error> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|source| file_error(folder, source))?;
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
        .map_err(|source| file_error(path, source))
}

pub(super) fn file_error(path: &Path, source: std::io::Error) -> Error {
    Error::File {
        path: path.to_owned(),
        source,
    }
}

/// Verifies `path` against lowercase hex SHA-256 `expected`. On mismatch,
/// deletes `path` so a retry fetches a clean copy.
pub(super) fn verify_sha256(path: &Path, expected: &str, cancel: &AtomicBool) -> Result<(), Error> {
    let mut file = File::open(path).map_err(|source| file_error(path, source))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let read = file
            .read(&mut buffer)
            .map_err(|source| file_error(path, source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut actual = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut actual, "{byte:02x}");
    }
    if !actual.eq_ignore_ascii_case(expected) {
        let _ = fs::remove_file(path);
        return Err(Error::ChecksumMismatch {
            expected: expected.to_ascii_lowercase(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
