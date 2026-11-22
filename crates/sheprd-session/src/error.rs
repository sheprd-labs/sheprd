use crate::{panes, saved, spaces};
use saved::edit;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Edit(#[from] edit::Error),
    #[error(transparent)]
    Panes(#[from] panes::Error),
    #[error(transparent)]
    Saved(#[from] saved::Error),
    #[error(transparent)]
    Spaces(#[from] spaces::Error),
    // keep-sorted end
}
