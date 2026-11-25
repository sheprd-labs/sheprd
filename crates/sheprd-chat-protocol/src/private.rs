//! Files only this user may read, written whole or not at all: what
//! restore records and threads keep on disk.

use std::{
    fs,
    io::{self, Read as _, Write as _},
    path::Path,
};

/// At most `limit + 1` bytes of the file at `path`: one more than the limit
/// tells the caller the file is larger than it accepts.
pub(crate) fn read_up_to(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Writes `bytes` to `path` through a temporary file in `dir` and a rename,
/// with the directory and file private to this user on Unix.
pub(crate) fn write(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    create_dir(dir)?;
    // Unique per write, and never an existing file: two windows of one app
    // may stamp the same chat at once.
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut temporary = path.to_owned();
    temporary.set_extension(format!("tmp-{}-{write}", std::process::id()));
    let written = open_new(&temporary)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Opens `path` to append to, creating it private to this user.
pub(crate) fn open_append(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

fn open_new(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

pub(crate) fn create_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}
