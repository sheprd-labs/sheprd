//! A phone's new workspace: in a folder, on a worktree when a branch is
//! given (a new branch, or a local one as it is), or in a checkout of the
//! repository that exists already, with a chat or a shell in it.
//!
//! A worktree of a repository Herdr 1 already shows is made by Herdr 1 from
//! that repository's workspace, as Sheprd's own new worktree is, so it
//! joins the repository's group in every client. Its chat starts in the
//! shell Herdr 1 opens there, as the desktop's does, so the workspace holds
//! the chat alone and no idle shell beside it. Any other new workspace goes
//! to the core, which Sheprd keeps a workspace for, or to Herdr 1 when no
//! core answers.

use super::{
    ApiMethod, CreatedWorkspace, Error, Hub, Launcher, Layout, Result, command_line, core,
    created_tab, direct_command_line, on_core, sessions, tab_params,
};
use crate::native::changes::local::git;
use serde_json::{Value, json};
use sheprd_chat_protocol::Agent;
use std::path::{Path, PathBuf};

/// What a phone's new workspace asks for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NewWorkspace<'a> {
    pub(crate) agent: Agent,
    /// Where a new `branch` starts, a full ref such as a fetched pull
    /// request's head; `None` for the checkout's own branch.
    pub(crate) base: Option<&'a str>,
    /// A worktree on this branch, else the folder itself. A branch that
    /// exists locally is checked out as it is.
    pub(crate) branch: Option<&'a str>,
    /// A checkout of `cwd`'s repository that exists already, opened in a
    /// workspace of its own instead; the caller checked it is one.
    pub(crate) checkout: Option<&'a str>,
    pub(crate) cwd: &'a str,
    /// The Herdr 1 session this daemon follows by that name; the primary
    /// otherwise.
    pub(crate) session: Option<&'a str>,
    /// A shell, not a chat with `agent`, in the new workspace.
    pub(crate) shell: bool,
    /// The name the person gave the workspace; else it is named for its
    /// folder or branch.
    pub(crate) title: Option<&'a str>,
}

/// What the new workspace's first pane runs: a chat, with the program
/// that starts it, or the checkout's shell, which needs no program.
#[derive(Clone, Debug)]
pub(super) enum Landing {
    Chat(Agent, Launcher),
    Shell,
}

impl Hub {
    /// Creates `new`, answering its workspace and its chat's pane. While a
    /// core answers the chat starts there.
    pub(crate) async fn create_workspace(&self, new: NewWorkspace<'_>) -> Result<CreatedWorkspace> {
        // As in [`Hub::start_chat`]: the program here cannot run in a
        // distribution, and a core would open the chat on this computer.
        if new.session.is_some_and(sessions::is_wsl) {
            return Err(Error::WslChat);
        }
        // A shell needs no chat program, so a computer without one can
        // still open a terminal.
        let landing = if new.shell {
            Landing::Shell
        } else {
            Landing::Chat(new.agent, self.launcher().await?)
        };
        let core = self.core_for_work().await?;
        if new.checkout.is_some() || new.branch.is_some() {
            let session = new.session.filter(|name| self.sessions.get(name).is_some());
            let cwd = PathBuf::from(new.cwd);
            // Git, not a workspace's folder, says which repository `cwd`
            // is: a workspace's folder follows its shell elsewhere.
            let layout = self.layout();
            let owner = self.core.clone();
            let session = session.map(str::to_owned);
            let repository = on_core(move || {
                Ok(Checkout::of(&cwd).and_then(|checkout| {
                    herdr_1_repository(&layout, &checkout, session.as_deref(), |id| {
                        owner.as_ref().is_some_and(|core| core.keeps_workspace(id))
                    })
                }))
            })
            .await?;
            match (repository, new.checkout, new.branch) {
                (Some(source), Some(path), _) => {
                    return self
                        .herdr_1_checkout(&source.workspace, path, new, &landing)
                        .await;
                }
                (Some(source), None, Some(branch)) => {
                    let worktree = Worktree {
                        base: new.base.or(source.base.as_deref()),
                        branch,
                        repository: &source.workspace,
                        title: new.title,
                    };
                    return self.herdr_1_worktree(worktree, &landing).await;
                }
                _ => {}
            }
        }
        // Elsewhere an existing checkout is a folder like any other.
        let new = match new.checkout {
            Some(path) => NewWorkspace {
                base: None,
                branch: None,
                checkout: None,
                cwd: path,
                ..new
            },
            None => new,
        };
        if let Some(core) = core {
            return self.core_workspace(core, new, &landing).await;
        }
        self.herdr_1_workspace(new, &landing).await
    }

    /// `new` on the core, in its unlisted group until Sheprd keeps a
    /// workspace for it: the core has no workspaces or worktrees, so a
    /// branch's checkout is made here.
    async fn core_workspace(
        &self,
        core: std::sync::Arc<core::Source>,
        new: NewWorkspace<'_>,
        landing: &Landing,
    ) -> Result<CreatedWorkspace> {
        // Herdr 2 keeps no Herdr 1 session: asked for a named one, the
        // workspace could not be where it was asked for.
        if new
            .session
            .is_some_and(|name| self.sessions.get(name).is_some())
        {
            return Err(Error::SessionOnHerdr2);
        }
        let (cwd, landing) = (new.cwd.to_owned(), landing.clone());
        let (branch, root) = (new.branch.map(str::to_owned), self.worktrees.clone());
        let base = new.base.unwrap_or("HEAD").to_owned();
        let title = new.title.map(str::to_owned);
        let (pane, folder, kept) = on_core(move || {
            let title = title.as_deref();
            let (pane, folder) = match branch {
                Some(branch) => core.open_worktree(
                    &landing,
                    core::NewCheckout {
                        repository: &cwd,
                        branch: &branch,
                        base: &base,
                        root: root.as_deref(),
                    },
                    title,
                ),
                // Sheprd keeps a workspace for it, in this folder.
                None => core
                    .open_landing(&landing, &cwd, Some(core::Place::New { cwd: &cwd, title }))
                    .map(|pane| (pane, cwd)),
            }?;
            let named = match &landing {
                Landing::Chat(agent, _) => agent.display_name(),
                Landing::Shell => SHELL_TITLE,
            };
            // Kept in the core's shared document at once, titled as the
            // phone shows it; its label still asks a desktop to keep it.
            let kept = core
                .share_new_workspace(&folder, &pane, title.unwrap_or(named))
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "A phone's new workspace waits for Sheprd to keep it");
                    None
                });
            Ok((pane, folder, kept))
        })
        .await?;
        // Otherwise listed apart until Sheprd keeps its workspace.
        Ok(CreatedWorkspace {
            workspace: kept.unwrap_or_else(|| core::new_workspace_id(&folder)),
            pane,
        })
    }

    /// A worktree Herdr 1 makes from `worktree.repository`'s workspace, in a
    /// workspace of its own there, with the chat typed into its first pane.
    ///
    /// Not on a core even while one answers: Herdr 1 always opens that
    /// shell, and a workspace's last tab cannot close without the workspace,
    /// so a chat in another tab would leave the shell idle beside it.
    async fn herdr_1_worktree(
        &self,
        worktree: Worktree<'_>,
        landing: &Landing,
    ) -> Result<CreatedWorkspace> {
        let created = self
            .call(ApiMethod::WorktreeCreate, worktree.params())
            .await?;
        self.land_in_root(&created, landing).await
    }

    /// A checkout of the repository of `repository`'s workspace that exists
    /// already, opened by Herdr 1 so it joins the repository's group. One
    /// some workspace has open already gets a new tab there instead.
    async fn herdr_1_checkout(
        &self,
        repository: &str,
        path: &str,
        new: NewWorkspace<'_>,
        landing: &Landing,
    ) -> Result<CreatedWorkspace> {
        let mut params = json!({"workspace_id": repository, "path": path, "focus": false});
        if let Some(title) = new.title {
            params["label"] = title.into();
        }
        let opened = self.call(ApiMethod::WorktreeOpen, params).await?;
        if opened["already_open"].as_bool() != Some(true) {
            return self.land_in_root(&opened, landing).await;
        }
        let workspace = answered(&opened["workspace"]["workspace_id"])?;
        let pane = match landing {
            Landing::Chat(agent, _) => self.start_chat(&workspace, *agent, None).await?,
            Landing::Shell => self.new_terminal(&workspace).await?,
        };
        Ok(CreatedWorkspace { workspace, pane })
    }

    /// The workspace Herdr 1 `answer`ed with its first pane, a shell in its
    /// folder, where a chat starts unless the shell is what was asked for.
    async fn land_in_root(&self, answer: &Value, landing: &Landing) -> Result<CreatedWorkspace> {
        let workspace = answered(&answer["workspace"]["workspace_id"])?;
        let pane = answered(&answer["root_pane"]["pane_id"])?;
        if let Landing::Chat(agent, launcher) = landing {
            // The program path rides in this line because the pane was not
            // made with it.
            self.run_line(&pane, &direct_command_line(launcher, *agent))
                .await
                .inspect_err(|error| {
                    tracing::warn!(%error, %workspace, "A new worktree's chat did not start; the worktree stays");
                })?;
        }
        Ok(CreatedWorkspace { workspace, pane })
    }

    /// `new` in Herdr 1, which no core stands in for: a workspace in its
    /// folder, and a worktree from it when a branch is given.
    async fn herdr_1_workspace(
        &self,
        new: NewWorkspace<'_>,
        landing: &Landing,
    ) -> Result<CreatedWorkspace> {
        let mut params = json!({"cwd": new.cwd, "focus": false});
        if let (Some(title), None) = (new.title, new.branch) {
            params["label"] = title.into();
        }
        let opened = self
            .call_in(new.session, ApiMethod::WorkspaceCreate, params)
            .await?;
        let workspace = answered(&opened["workspace"]["workspace_id"])?;
        let Some(branch) = new.branch else {
            let Landing::Chat(agent, launcher) = landing else {
                let pane = answered(&opened["root_pane"]["pane_id"])?;
                return Ok(CreatedWorkspace { workspace, pane });
            };
            let response = self
                .call(
                    ApiMethod::TabCreate,
                    tab_params(&workspace, new.cwd, *agent, &launcher.program),
                )
                .await?;
            let (_, pane) = created_tab(&response).ok_or(Error::Response)?;
            self.run_line(&pane, &command_line(launcher, *agent))
                .await?;
            return Ok(CreatedWorkspace { workspace, pane });
        };
        let worktree = Worktree {
            base: new.base,
            branch,
            repository: &workspace,
            title: new.title,
        };
        self.herdr_1_worktree(worktree, landing).await
    }
}

/// What a new workspace with a shell is called until Sheprd names it.
const SHELL_TITLE: &str = "Terminal";

/// A new worktree Herdr 1 makes.
#[derive(Clone, Copy, Debug)]
struct Worktree<'a> {
    /// The full ref it starts from; `None` for the repository's `HEAD`.
    base: Option<&'a str>,
    branch: &'a str,
    /// The workspace of the repository it is made from.
    repository: &'a str,
    title: Option<&'a str>,
}

impl Worktree<'_> {
    /// Its `worktree.create` parameters; a title names the new workspace.
    fn params(self) -> Value {
        let mut params = json!({
            "workspace_id": self.repository,
            "branch": self.branch,
            "focus": false,
        });
        if let Some(title) = self.title {
            params["label"] = title.into();
        }
        if let Some(base) = self.base {
            params["base"] = base.into();
        }
        params
    }
}

/// A non-empty id or path Herdr answered.
fn answered(value: &Value) -> Result<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or(Error::Response)
}

/// Where Herdr 1 makes a new worktree from: the workspace of the
/// repository's own checkout, and the ref it starts from when that is not
/// the checkout's `HEAD`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Source {
    pub(super) workspace: String,
    pub(super) base: Option<String>,
}

/// The checkout a folder is in, as Git and Herdr 1 name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Checkout {
    /// Its repository's Git directory, which Herdr 1 keys workspaces by.
    pub(super) common_dir: PathBuf,
    /// A linked worktree, not the repository's own checkout.
    pub(super) linked: bool,
    /// Its branch's full ref; `None` when detached.
    pub(super) branch: Option<String>,
}

impl Checkout {
    /// The checkout `cwd` is in, `None` outside one. Runs Git: call it
    /// off the async workers.
    fn of(cwd: &Path) -> Option<Self> {
        let dirs = git(
            cwd,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
                "--git-dir",
            ],
        )
        .ok()?;
        let branch = git(cwd, &["symbolic-ref", "--quiet", "HEAD"]).ok();
        Self::parse(&dirs, branch.as_deref(), canonical_path)
    }

    /// From `rev-parse`'s common and Git directories and `symbolic-ref`'s
    /// branch, with paths made canonical as Herdr 1's keys are.
    fn parse(
        dirs: &str,
        branch: Option<&str>,
        canonical: impl Fn(&Path) -> PathBuf,
    ) -> Option<Self> {
        let mut lines = dirs.lines().map(str::trim).filter(|line| !line.is_empty());
        let common_dir = canonical(Path::new(lines.next()?));
        let git_dir = canonical(Path::new(lines.next()?));
        let branch = branch
            .map(str::trim)
            .filter(|branch| branch.starts_with("refs/heads/"))
            .map(str::to_owned);
        Some(Self {
            linked: git_dir != common_dir,
            common_dir,
            branch,
        })
    }
}

fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// Where Herdr 1 makes a new worktree in `checkout` from, among
/// `session`'s workspaces (the primary's when `None`): the workspace of
/// its repository's own checkout, keyed by its Git directory, and for a
/// linked worktree its branch, as the desktop's new worktree does: Herdr 1
/// refuses a linked checkout as the source.
///
/// `None` when Herdr 1 shows no such checkout, and the core makes it as
/// before. Workspaces Sheprd keeps for Herdr 2, a core's new workspaces,
/// and WSL's are not Herdr 1's to grow here.
pub(super) fn herdr_1_repository(
    layout: &Layout,
    checkout: &Checkout,
    session: Option<&str>,
    kept: impl Fn(&str) -> bool,
) -> Option<Source> {
    let base = match (checkout.linked, &checkout.branch) {
        (false, _) => None,
        (true, Some(branch)) => Some(branch.clone()),
        (true, None) => return None,
    };
    let mut eligible = layout.workspaces.iter().filter(|workspace| {
        sessions::split_id(&workspace.id).0 == session
            && !sessions::in_wsl(&workspace.id)
            && !core::is_new_workspace(&workspace.id)
            && workspace.id != core::UNLISTED
            && !kept(&workspace.id)
    });
    let workspace = eligible
        .clone()
        .find(|workspace| {
            workspace.worktree.as_ref().is_some_and(|tree| {
                !tree.is_linked_worktree && Path::new(&tree.key) == checkout.common_dir
            })
        })
        .or_else(|| {
            // Before the first worktree, Herdr can omit repository metadata.
            // worktree.create discovers Git and groups the original workspace too.
            eligible.find(|workspace| {
                workspace.worktree.is_none()
                    && !workspace.cwd.is_empty()
                    && canonical_path(&Path::new(&workspace.cwd).join(".git"))
                        == checkout.common_dir
            })
        })?;
    Some(Source {
        workspace: workspace.id.clone(),
        base,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
