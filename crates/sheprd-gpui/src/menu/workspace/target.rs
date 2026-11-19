//! What a workspace menu row targets: the workspace itself, how worktree
//! requests name its repository, the siblings that close with it, and why the
//! new worktree shortcut may find no source.

use super::popover::Label;
use crate::i18n::fl;
use crate::menu::WorkspaceAction;
use sheprd_client::{
    Method,
    protocol::{ClientShellSnapshot, ClientShellWorkspace, ClientShellWorktree},
};

pub(crate) struct WorkspaceTarget {
    pub(in crate::menu) boot_id: String,
    pub(in crate::menu) id: String,
    pub(in crate::menu) label: String,
    /// The folder the reveal action opens; only macOS and Linux offer it.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
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

/// How a worktree request tells the daemon which repository it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::menu) enum Source {
    /// By workspace: the daemon branches the repository it keeps for it.
    Workspace,
    /// By directory, which the daemon resolves to its repository. Herdr keeps
    /// the repository a workspace was first given while its directory, and the
    /// label drawn from it, follow the shell: a workspace Teleport opened on
    /// one repository whose shell then moved into another would otherwise
    /// branch the first one under the second one's name.
    Directory(String),
    /// A repository's main checkout directory, for a linked checkout whose
    /// only open main checkout's shell has wandered off: the daemon refuses
    /// a linked checkout as a source. Its Git directory, with the linked
    /// checkout's branch, is what lookups read, since the main checkout's
    /// reported branch is of the repository its shell moved to.
    Repository { root: String, key: String },
}

impl Source {
    fn of(workspace: &ClientShellWorkspace) -> Self {
        if left_checkout(workspace) {
            Self::Directory(workspace.new_workspace_cwd.clone())
        } else {
            Self::Workspace
        }
    }
}

impl WorkspaceTarget {
    pub(in crate::menu) fn new(
        snapshot: &ClientShellSnapshot,
        workspace: &ClientShellWorkspace,
    ) -> Self {
        Self {
            boot_id: snapshot.boot_id.clone(),
            id: workspace.workspace_id.clone(),
            label: workspace.label.clone(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            cwd: workspace.new_workspace_cwd.clone(),
            worktree: known_worktree(workspace),
            close_members: close_members(snapshot, workspace),
            heads_group: heads_group(snapshot, workspace),
            branch: workspace.branch.clone(),
            base: None,
            source: Source::of(workspace),
            keeper: sheprd_session::Keeper::Herdr1,
            muted: false,
            retained: false,
        }
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
            #[cfg(any(target_os = "macos", target_os = "linux"))]
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

    /// Whether `workspace` is still named the way this target names it. A
    /// repository root stands for its main checkout only while that main
    /// checkout is still away from it.
    pub(in crate::menu) fn addresses(&self, workspace: &ClientShellWorkspace) -> bool {
        match (&self.source, Source::of(workspace)) {
            (Source::Repository { .. }, Source::Directory(_)) => true,
            (source, current) => *source == current,
        }
    }

    /// The fields naming the repository a worktree request is for.
    fn source_params(&self) -> serde_json::Value {
        match &self.source {
            Source::Workspace => serde_json::json!({"workspace_id": self.id}),
            Source::Directory(cwd) | Source::Repository { root: cwd, .. } => {
                serde_json::json!({"cwd": cwd})
            }
        }
    }

    /// The `worktree.list` that fills this target's dialogs.
    pub(in crate::menu) fn list_params(&self) -> serde_json::Value {
        let mut params = self.source_params();
        params["trust_repository"] = false.into();
        params
    }

    /// The target a new worktree for `workspace` is created through. A linked
    /// checkout resolves to its repository's open main checkout, based on the
    /// linked checkout's branch so the new branch starts where it stands.
    pub(in crate::menu) fn for_new_worktree(
        snapshot: &ClientShellSnapshot,
        workspace: &ClientShellWorkspace,
    ) -> Result<Self, NewWorktreeUnavailable> {
        let Some(tree) = workspace
            .worktree
            .as_ref()
            .filter(|tree| tree.is_linked_worktree)
        else {
            let target = Self::new(snapshot, workspace);
            return if target.can_create() {
                Ok(target)
            } else {
                Err(NewWorktreeUnavailable::NotGit)
            };
        };
        let branch = workspace
            .branch
            .clone()
            .ok_or(NewWorktreeUnavailable::Detached)?;
        let main = main_checkout(snapshot, tree)
            .or_else(|| snapshot.workspaces.iter().find(|w| is_main_of(w, tree)))
            .ok_or(NewWorktreeUnavailable::MainCheckoutClosed)?;
        let mut target = Self {
            base: Some(branch),
            ..Self::new(snapshot, main)
        };
        // The linked checkout still names its repository. When the main
        // checkout's shell has wandered off, its directory is named instead,
        // since the daemon refuses a linked checkout as the source.
        if let (Source::Directory(_), Some(root)) =
            (&target.source, crate::worktree::checkout_root(&tree.key))
        {
            target.source = Source::Repository {
                root: root.to_owned(),
                key: tree.key.clone(),
            };
        }
        Ok(target)
    }

    /// The ref a new worktree starts from: a full ref, so a tag of the same
    /// name cannot shadow the branch.
    pub(in crate::menu) fn base_ref(&self) -> crate::Result<String> {
        match &self.base {
            Some(branch) => {
                crate::worktree::validate_branch(branch)?;
                Ok(format!("refs/heads/{branch}"))
            }
            None => Ok("HEAD".to_owned()),
        }
    }

    /// What the new worktree dialog says it branches from.
    pub(in crate::menu) fn base_label(&self) -> &str {
        self.base.as_deref().unwrap_or("HEAD")
    }

    pub(in crate::menu) fn can_create(&self) -> bool {
        // Like the TUI, accept Git branches before worktree metadata is available.
        // A repository root is named by its linked checkout, whatever the
        // moved main checkout's directory reports.
        (self.worktree.is_some()
            || self.branch.is_some()
            || matches!(self.source, Source::Repository { .. }))
            && !self.can_delete()
    }

    pub(in crate::menu) fn can_delete(&self) -> bool {
        self.worktree
            .as_ref()
            .is_some_and(|tree| tree.is_linked_worktree)
    }

    pub(in crate::menu) fn validate_repository(
        &self,
        snapshot: &ClientShellSnapshot,
    ) -> crate::Result<()> {
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == self.id)
            .filter(|_| snapshot.boot_id == self.boot_id)
            .ok_or(crate::Error::StaleWorkspace)?;
        let worktree = known_worktree(workspace);
        let named = match &self.source {
            // The main checkout must still be kept for the same repository;
            // the branch of the directory its shell moved to is unrelated.
            Source::Repository { key, .. } => workspace
                .worktree
                .as_ref()
                .is_some_and(|tree| tree.key == *key && !tree.is_linked_worktree),
            _ => worktree.is_some() || workspace.branch.is_some(),
        };
        if !self.can_create() || !self.addresses(workspace) || self.worktree != worktree || !named {
            return Err(crate::Error::WorkspaceRepositoryChanged);
        }
        Ok(())
    }

    /// The worktree key this workspace heads, when linked checkouts hang off it.
    pub(in crate::menu) fn group_key(&self) -> Option<&str> {
        self.worktree
            .as_ref()
            .filter(|_| self.heads_group)
            .map(|tree| tree.key.as_str())
    }

    pub(in crate::menu) fn close_label(&self) -> Label {
        if self.close_members.len() > 1 {
            Label::CLOSE_GROUP
        } else {
            Label::CLOSE
        }
    }

    pub(in crate::menu) fn request(
        &self,
        snapshot: &ClientShellSnapshot,
        action: WorkspaceAction,
        text: &str,
    ) -> crate::Result<(Method, serde_json::Value)> {
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|w| w.workspace_id == self.id)
            .filter(|_| snapshot.boot_id == self.boot_id)
            .ok_or(crate::Error::StaleWorkspace)?;
        let params = match action {
            WorkspaceAction::Rename => {
                let label = text.trim();
                if label.is_empty() {
                    return Err(crate::Error::EmptyWorkspaceLabel);
                }
                (
                    Method::WorkspaceRename,
                    serde_json::json!({"workspace_id": self.id, "label": label}),
                )
            }
            WorkspaceAction::Close => {
                if self.worktree != known_worktree(workspace)
                    || self.close_members != close_members(snapshot, workspace)
                {
                    return Err(crate::Error::WorkspaceGroupChanged);
                }
                // The daemon closes every checkout of the repository for a
                // group close, so a parent beside another parent asks for
                // itself alone.
                (
                    Method::WorkspaceClose,
                    serde_json::json!({"workspace_id": self.id, "close_group": self.close_members.len() > 1}),
                )
            }
            WorkspaceAction::NewWorktree => {
                self.validate_repository(snapshot)?;
                let base = self.base_ref()?;
                let mut params = self.source_params();
                params["base"] = base.into();
                params["focus"] = true.into();
                params["trust_repository"] = false.into();
                if !text.trim().is_empty() {
                    crate::worktree::validate_branch(text.trim())?;
                    params["branch"] = text.trim().into();
                }
                (Method::WorktreeCreate, params)
            }
            WorkspaceAction::OpenWorktree => {
                self.validate_repository(snapshot)?;
                if text.is_empty() {
                    return Err(crate::Error::WorktreeSelection);
                }
                let mut params = self.source_params();
                params["path"] = text.into();
                params["focus"] = true.into();
                params["trust_repository"] = false.into();
                (Method::WorktreeOpen, params)
            }
            // The label, when there is one, is added by the dialog that knows
            // which name it proposed.
            WorkspaceAction::NewTab => (
                Method::TabCreate,
                serde_json::json!({"workspace_id": self.id, "focus": true}),
            ),
            WorkspaceAction::NewWorkspace => (
                Method::WorkspaceCreate,
                serde_json::json!({"focus": true, "source_workspace_id": self.id}),
            ),
            WorkspaceAction::Note => return Err(crate::Error::LocalWorktreeNote),
            WorkspaceAction::DeleteWorktree => {
                if !self.can_delete() || self.worktree != known_worktree(workspace) {
                    return Err(crate::Error::WorkspaceCheckoutChanged);
                }
                (
                    Method::WorktreeRemove,
                    serde_json::json!({"workspace_id": self.id, "force": false, "trust_repository": false}),
                )
            }
        };
        Ok(params)
    }
}

/// Why the new worktree shortcut found no workspace to branch from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::menu) enum NewWorktreeUnavailable {
    Disconnected,
    NoWorkspace,
    NotGit,
    MainCheckoutClosed,
    Detached,
}

impl NewWorktreeUnavailable {
    pub(in crate::menu) fn message(self) -> String {
        match self {
            Self::Disconnected => fl!("worktree-popover-new-disconnected"),
            Self::NoWorkspace => fl!("worktree-popover-new-no-workspace"),
            Self::NotGit => fl!("worktree-popover-not-git"),
            Self::MainCheckoutClosed => fl!("worktree-popover-new-main-closed"),
            Self::Detached => fl!("worktree-popover-new-detached"),
        }
    }
}

/// Whether `workspace` is a main checkout whose directory is no longer inside
/// the repository Herdr keeps for it. A linked checkout's own path is not in
/// the snapshot, so it is never judged.
fn left_checkout(workspace: &ClientShellWorkspace) -> bool {
    workspace.worktree.as_ref().is_some_and(|tree| {
        !tree.is_linked_worktree
            && crate::worktree::left_checkout(&tree.key, &workspace.new_workspace_cwd)
    })
}

/// The repository `workspace` is in, as far as it can be trusted: none for a
/// main checkout whose shell has left the repository Herdr keeps for it.
pub(in crate::menu) fn known_worktree(
    workspace: &ClientShellWorkspace,
) -> Option<ClientShellWorktree> {
    workspace
        .worktree
        .clone()
        .filter(|_| !left_checkout(workspace))
}

/// The open main checkout of `tree`'s repository, through which the daemon
/// lists, opens, and creates that repository's worktrees. One whose shell has
/// left the repository no longer stands for it.
pub(in crate::menu) fn main_checkout<'a>(
    snapshot: &'a ClientShellSnapshot,
    tree: &ClientShellWorktree,
) -> Option<&'a ClientShellWorkspace> {
    snapshot
        .workspaces
        .iter()
        .find(|w| is_main_of(w, tree) && !left_checkout(w))
}

/// Whether the daemon keeps `workspace` as `tree`'s main checkout, wherever
/// its shell is now.
pub(in crate::menu) fn is_main_of(
    workspace: &ClientShellWorkspace,
    tree: &ClientShellWorktree,
) -> bool {
    workspace
        .worktree
        .as_ref()
        .is_some_and(|other| other.key == tree.key && !other.is_linked_worktree)
}

/// The workspaces closing `workspace` closes: its whole group when it is its
/// repository's only parent and linked worktrees hang off it, else itself.
/// Like the TUI, a parent beside another parent closes alone.
fn close_members(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> Vec<String> {
    let alone = || vec![workspace.workspace_id.clone()];
    let Some(tree) = workspace
        .worktree
        .as_ref()
        .filter(|tree| !tree.is_linked_worktree)
    else {
        return alone();
    };
    let mut members = Vec::new();
    for w in &snapshot.workspaces {
        match &w.worktree {
            Some(other) if other.key == tree.key => {
                if w.workspace_id != workspace.workspace_id && !other.is_linked_worktree {
                    return alone();
                }
                members.push(w.workspace_id.clone());
            }
            _ => {}
        }
    }
    members.sort();
    members
}

/// Whether `workspace` is a parent with linked worktrees of its repository open.
fn heads_group(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> bool {
    workspace
        .worktree
        .as_ref()
        .filter(|tree| !tree.is_linked_worktree)
        .is_some_and(|tree| {
            snapshot.workspaces.iter().any(|w| {
                w.worktree
                    .as_ref()
                    .is_some_and(|other| other.key == tree.key && other.is_linked_worktree)
            })
        })
}
