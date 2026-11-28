use super::Source;
use sheprd_client::protocol::ClientShellWorktree;

pub(crate) struct WorkspaceTarget {
    pub(in crate::menu) boot_id: String,
    pub(in crate::menu) id: String,
    pub(in crate::menu) label: String,
    #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
    pub(in crate::menu) cwd: String,
    pub(in crate::menu) worktree: Option<ClientShellWorktree>,
    pub(in crate::menu) close_members: Vec<String>,
    /// Whether linked worktrees of this parent's repository are open, so its
    /// row folds a group even when another parent closes on its own.
    pub(in crate::menu) heads_group: bool,
    pub(in crate::menu) branch: Option<String>,
    /// The branch a new worktree starts from, when it is not this checkout's
    /// `HEAD`: a linked checkout asks its main checkout, the only source the
    /// daemon accepts, to branch from the linked checkout's branch instead.
    pub(in crate::menu) base: Option<String>,
    /// How worktree requests name the repository to the daemon.
    pub(in crate::menu) source: Source,
    /// Who carries out what the menu asks: Herdr 1's daemon, or the app for
    /// a workspace it keeps for Herdr 2 (`kept.rs`).
    pub(in crate::menu) keeper: sheprd_session::Keeper,
    /// A remembered folder with no live daemon workspace behind its row.
    pub(in crate::menu) retained: bool,
    /// Whether the user muted the workspace's notifications when the menu
    /// opened, which picks what the toggle in its header does.
    pub(in crate::menu) muted: bool,
}

impl WorkspaceTarget {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// Who renames it: Herdr 1's daemon, or the app.
    pub(crate) fn keeper(&self) -> sheprd_session::Keeper {
        self.keeper
    }

    /// A saved project row, whose folder is available without a live workspace.
    pub(in crate::menu) fn retained(
        id: String,
        label: String,
        cwd: String,
        branch: Option<String>,
    ) -> Self {
        Self {
            boot_id: String::new(),
            id,
            label,
            cwd: cwd.clone(),
            worktree: None,
            close_members: Vec::new(),
            heads_group: false,
            branch,
            base: None,
            source: Source::Directory(cwd),
            keeper: sheprd_session::Keeper::Herdr1,
            muted: false,
            retained: true,
        }
    }
}
