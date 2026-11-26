#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("no private folder holds pasted files")]
    Folder,
    #[error("could not save the pasted file")]
    Write(#[source] std::io::Error),
    // keep-sorted end
}
