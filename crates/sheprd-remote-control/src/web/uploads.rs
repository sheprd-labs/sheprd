//! Pictures a phone attaches to a prompt. The chat program reads
//! attachments by path on this computer, which a phone must never name, so
//! an upload is checked to be an image by its bytes, written 0600 to a
//! private folder under a random name, and known to the phone only by an
//! opaque id that a prompt's attachment carries. A file dropped on a
//! terminal is any kind, kept under its name made safe, its path typed by
//! this computer rather than named by the phone. Uploads are forgotten and
//! deleted after a day, and only a bounded number are kept.

use sheprd_chat_protocol::{Attachment, ImageType};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

/// The largest picture a phone may send; the chat program's own image
/// limit is 5 MiB of base64, which this stays under.
pub(crate) const MAX_UPLOAD: usize = 3_500_000;
/// The largest file a phone may drop on a terminal; kept modest, as up to
/// [`MAX_UPLOADS`] of them stay on disk for a day.
pub(crate) const MAX_FILE: usize = 16 * 1024 * 1024;
/// The most UTF-8 bytes of a file's name kept in its stored name. File
/// systems limit a name's bytes, often to 255, and the id and `-` go first.
const MAX_NAME: usize = 100;
/// Uploads kept at once.
const MAX_UPLOADS: usize = 64;
/// How long an upload stays attachable.
const LIFETIME_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("the file is larger than {MAX_FILE} bytes")]
    FileTooLarge,
    #[error("that is not a PNG, JPEG, GIF, or WebP picture")]
    NotAnImage,
    #[error("the picture is larger than {MAX_UPLOAD} bytes")]
    TooLarge,
    #[error("that attachment is no longer available; attach it again")]
    UnknownUpload,
    #[error("could not store the picture")]
    Write(#[source] std::io::Error),
    // keep-sorted end
}

struct Upload {
    path: PathBuf,
    created_ms: u64,
}

pub(crate) struct Uploads {
    dir: PathBuf,
    uploads: Mutex<HashMap<String, Upload>>,
}

/// The kind of picture `bytes` hold, by their signature.
pub(crate) fn sniff(bytes: &[u8]) -> Option<ImageType> {
    match bytes {
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some(ImageType::Png),
        [0xff, 0xd8, 0xff, ..] => Some(ImageType::Jpeg),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(ImageType::Gif),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(ImageType::Webp),
        _ => None,
    }
}

fn extension(kind: ImageType) -> &'static str {
    match kind {
        ImageType::Gif => "gif",
        ImageType::Jpeg => "jpg",
        ImageType::Png => "png",
        ImageType::Webp => "webp",
    }
}

impl Uploads {
    pub(crate) fn new(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.join("uploads"),
            uploads: Mutex::new(HashMap::new()),
        }
    }

    /// Stores `bytes` and returns the id a prompt attaches. Blocking.
    pub(crate) fn store(&self, id: String, bytes: &[u8], now_ms: u64) -> Result<String, Error> {
        if bytes.len() > MAX_UPLOAD {
            return Err(Error::TooLarge);
        }
        let kind = sniff(bytes).ok_or(Error::NotAnImage)?;
        let name = format!("{id}.{}", extension(kind));
        self.keep(id, &name, bytes, now_ms)
    }

    /// Stores any file a phone drops on a terminal under the name it had,
    /// made safe and prefixed by `id`, so the program reading the path sees
    /// its kind. Returns its id. Blocking.
    pub(crate) fn store_file(
        &self,
        id: String,
        name: &str,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<String, Error> {
        if bytes.len() > MAX_FILE {
            return Err(Error::FileTooLarge);
        }
        let name = format!("{id}-{}", file_name(name));
        self.keep(id, &name, bytes, now_ms)
    }

    /// Writes `bytes` as `name` and remembers it by `id`, dropping the
    /// oldest upload past the bound.
    fn keep(&self, id: String, name: &str, bytes: &[u8], now_ms: u64) -> Result<String, Error> {
        self.prune(now_ms);
        private_dir(&self.dir).map_err(Error::Write)?;
        let path = self.dir.join(name);
        write_private(&path, bytes).map_err(Error::Write)?;
        let mut uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
        if uploads.len() >= MAX_UPLOADS
            && let Some(oldest) = uploads
                .iter()
                .min_by_key(|(_, upload)| upload.created_ms)
                .map(|(id, _)| id.clone())
            && let Some(upload) = uploads.remove(&oldest)
        {
            let _ = std::fs::remove_file(upload.path);
        }
        uploads.insert(
            id.clone(),
            Upload {
                path,
                created_ms: now_ms,
            },
        );
        Ok(id)
    }

    /// The attachments a phone named by id, as the paths the chat program
    /// reads. An upload past its lifetime is gone even before a sweep.
    pub(crate) fn resolve(
        &self,
        attachments: &[Attachment],
        now_ms: u64,
    ) -> Result<Vec<Attachment>, Error> {
        attachments
            .iter()
            .map(|attachment| {
                self.path(&attachment.path, now_ms).map(|path| Attachment {
                    path: path.to_string_lossy().into_owned(),
                })
            })
            .collect()
    }

    /// Where the upload `id` names was written, while it is still kept.
    pub(crate) fn path(&self, id: &str, now_ms: u64) -> Result<PathBuf, Error> {
        let uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
        uploads
            .get(id)
            .filter(|upload| now_ms.saturating_sub(upload.created_ms) < LIFETIME_MS)
            .map(|upload| upload.path.clone())
            .ok_or(Error::UnknownUpload)
    }

    /// Deletes every picture an earlier run left: their ids died with it,
    /// so nothing can attach them. Blocking; run before serving.
    pub(crate) fn remove_leftovers(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Forgets and deletes uploads older than a day. Blocking.
    pub(crate) fn prune(&self, now_ms: u64) {
        let mut uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
        uploads.retain(|_, upload| {
            let keep = now_ms.saturating_sub(upload.created_ms) < LIFETIME_MS;
            if !keep {
                let _ = std::fs::remove_file(&upload.path);
            }
            keep
        });
    }
}

/// The last part of a name a phone sent, kept to letters, digits, `.`,
/// `-`, and `_`, never hidden or empty, and at most [`MAX_NAME`] bytes,
/// keeping its extension and whole characters.
pub(crate) fn file_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let safe: String = last
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_start_matches('.');
    if safe.is_empty() {
        return "file".to_owned();
    }
    if safe.len() <= MAX_NAME {
        return safe.to_owned();
    }
    // Over the limit: the start and the extension, which says the kind.
    let extension = safe
        .rfind('.')
        .map(|dot| &safe[dot..])
        .filter(|extension| extension.len() <= 16)
        .unwrap_or_default();
    let room = MAX_NAME - extension.len();
    let end = safe
        .char_indices()
        .map(|(at, c)| at + c.len_utf8())
        .take_while(|&end| end <= room)
        .last()
        .unwrap_or(0);
    format!("{}{extension}", &safe[..end])
}

fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
