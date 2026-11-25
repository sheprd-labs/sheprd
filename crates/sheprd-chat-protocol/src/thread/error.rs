use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("not a thread id")]
    Id,
    #[error("could not read or write a thread")]
    Io(#[from] io::Error),
    #[error("a thread record is not valid JSON")]
    Json(#[from] serde_json::Error),
    #[error("this thread already has a writer")]
    Locked,
    #[error(
        "a thread segment's model name exceeds {} bytes",
        crate::thread::MAX_MODEL
    )]
    Model,
    #[error("a thread record's folder is not an absolute path")]
    Path,
    #[error(
        "a thread has no segments or more than {}",
        crate::thread::MAX_SEGMENTS
    )]
    Segments,
    #[error("a thread segment's session id is not plain enough to type into a shell")]
    Session,
    #[error("a thread record exceeds {} bytes", crate::thread::MAX_RECORD_SIZE)]
    Size,
    #[error("a thread record's title exceeds {} bytes", crate::thread::MAX_TITLE)]
    Title,
    #[error(
        "a thread's transcript file exceeds {} bytes",
        crate::thread::file::MAX_FILE
    )]
    TranscriptSize,
    #[error("a thread record has version {0}")]
    Version(u32),
    // keep-sorted end
}
