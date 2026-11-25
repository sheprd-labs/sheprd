//! Automatic root-space names from the same live folder used for launch and metadata.

use crate::core_terminals::checkouts::Checkouts;
use sheprd_session::{FolderStyle, Session};

pub(super) fn apply(
    session: &mut Session,
    checkouts: Option<&Checkouts>,
    home: &str,
    style: FolderStyle,
) {
    for workspace in &mut session.workspaces {
        if workspace.custom_label || workspace.worktree.as_ref().is_some_and(|tree| tree.linked) {
            continue;
        }
        let cwd = workspace.cwd.as_str();
        let folder = checkouts
            .and_then(|known| known.get(cwd))
            .map(|checkout| checkout.root.as_str())
            .filter(|root| !root.is_empty())
            .unwrap_or(cwd);
        workspace.label = workspace.label_for_folder(folder, home, style);
    }
}

#[cfg(test)]
mod tests;
