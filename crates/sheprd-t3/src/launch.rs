//! Starting a thread from scratch on a running T3 Code server.
//!
//! A thread runs in a project, T3's name for a folder on its machine. The
//! project for a checkout is the one whose root is the checkout's
//! repository, or a new one T3 is asked to create (`projects.mutate`,
//! `rpc.ts:1251`, as T3's own client does: `client-runtime`'s
//! `createProject`). The thread then starts with
//! `orchestration.launchThread` (`orchestrationV2.ts:3248`): in the
//! repository itself, in a linked worktree that already exists, or in a new
//! worktree T3 makes from a base ref. Sheprd never starts T3: this only
//! talks to a server that already answers.

mod error;

pub use error::Error;

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    connection::Connection,
    frame::ServerFrame,
    shell::{Project, Shell},
    steer::{self, Provider, Source},
    thread::ModelSelection,
};

/// The RPC that creates a project.
pub const MUTATE_PROJECT: &str = "projects.mutate";
/// The RPC that starts a thread.
pub const LAUNCH: &str = "orchestration.launchThread";

/// The most of a prompt a thread's title keeps.
const TITLE_CHARS: usize = 80;

/// Where a checkout sits: its repository's main folder, and the linked
/// worktree it is, when it is one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkout {
    pub root: PathBuf,
    pub worktree: Option<PathBuf>,
}

/// The checkout holding `path`: the nearest folder above it with a `.git`.
/// A `.git` folder makes that folder the root; a `.git` file, which a linked
/// worktree has, points at the repository's common folder, whose parent is
/// the root. A folder in no repository is a root of its own. Read from the
/// files alone, with no `git` process.
pub fn checkout(path: &Path) -> Result<Checkout, Error> {
    let path = fs::canonicalize(path).map_err(|source| Error::Checkout {
        path: path.to_owned(),
        source,
    })?;
    for folder in path.ancestors() {
        let dot_git = folder.join(".git");
        let Ok(metadata) = fs::metadata(&dot_git) else {
            continue;
        };
        if metadata.is_dir() {
            return Ok(Checkout {
                root: folder.to_owned(),
                worktree: None,
            });
        }
        let gitdir = gitdir(&dot_git).ok_or_else(|| Error::Worktree {
            path: folder.to_owned(),
        })?;
        // A submodule, or a checkout made with `--separate-git-dir`, keeps
        // its Git folder elsewhere without being a linked worktree: it has
        // no `commondir`, and the folder is its own root.
        let Some(root) = linked_root(&gitdir) else {
            return Ok(Checkout {
                root: folder.to_owned(),
                worktree: None,
            });
        };
        return Ok(Checkout {
            root,
            worktree: Some(folder.to_owned()),
        });
    }
    Ok(Checkout {
        root: path,
        worktree: None,
    })
}

/// The Git folder a `.git` file names (`gitdir: PATH`), if it exists.
fn gitdir(dot_git: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(dot_git).ok()?;
    let gitdir = Path::new(text.strip_prefix("gitdir:")?.trim());
    let gitdir = dot_git.parent()?.join(gitdir);
    gitdir.is_dir().then_some(gitdir)
}

/// The main folder of the repository whose linked worktree keeps its Git
/// folder at `gitdir` (`<common>/worktrees/<name>`): its `commondir` leads
/// to `<common>`, the main folder's `.git`. None for a Git folder that is
/// no linked worktree's.
fn linked_root(gitdir: &Path) -> Option<PathBuf> {
    let common = fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common = fs::canonicalize(gitdir.join(common.trim())).ok()?;
    Some(common.parent()?.to_owned())
}

/// The project whose root is `root`, the same folder however written.
pub fn project_for<'a>(shell: &'a Shell, root: &Path) -> Option<&'a Project> {
    shell.projects.values().find(|project| {
        let theirs = Path::new(&project.workspace_root);
        theirs == root || fs::canonicalize(theirs).is_ok_and(|theirs| theirs == root)
    })
}

/// How the thread's agent asks before acting (`providerPolicy.ts:25`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeMode {
    ApprovalRequired,
    AutoAcceptEdits,
    Auto,
    /// T3's default.
    #[default]
    FullAccess,
}

impl std::str::FromStr for RuntimeMode {
    type Err = UnknownMode;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "approval-required" => Ok(Self::ApprovalRequired),
            "auto-accept-edits" => Ok(Self::AutoAcceptEdits),
            "auto" => Ok(Self::Auto),
            "full-access" => Ok(Self::FullAccess),
            _ => Err(UnknownMode),
        }
    }
}

/// A runtime mode T3 does not have.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("the mode is one of approval-required, auto-accept-edits, auto, or full-access")]
pub struct UnknownMode;

/// Whether the agent plans before it acts (`providerPolicy.ts:34`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    #[default]
    Default,
    Plan,
}

/// Where the thread works (`orchestrationV2.ts:570`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Workspace {
    /// The repository's main folder.
    Root,
    /// A linked worktree that already exists, such as one Sheprd made.
    #[serde(rename_all = "camelCase")]
    ExistingWorktree { worktree_path: String },
    /// A new worktree T3 makes from `base_ref`, on `branch` if named.
    #[serde(rename_all = "camelCase")]
    Worktree {
        base_ref: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
}

/// A new worktree for the thread, made by T3.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewWorktree {
    pub base: String,
    pub branch: Option<String>,
}

/// What to start.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Request {
    pub cwd: PathBuf,
    pub interaction_mode: InteractionMode,
    /// The model, by slug; the provider's default when left out.
    pub model: Option<String>,
    pub prompt: Option<String>,
    /// The provider instance; the project's, else the first ready one.
    pub provider: Option<String>,
    pub runtime_mode: RuntimeMode,
    /// The thread's title; by default T3 names it from the prompt.
    pub title: Option<String>,
    pub worktree: Option<NewWorktree>,
}

/// The `projects.mutate` payload that creates a project (`project.ts:208`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum ProjectMutation {
    #[serde(rename = "project.create", rename_all = "camelCase")]
    Create {
        command_id: String,
        project_id: String,
        title: String,
        workspace_root: String,
    },
}

/// The `orchestration.launchThread` payload (`orchestrationV2.ts:3248`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub command_id: String,
    pub creation_source: Source,
    pub generate_title: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_message: Option<InitialMessage>,
    pub interaction_mode: InteractionMode,
    pub model_selection: ModelSelection,
    pub project_id: String,
    pub runtime_mode: RuntimeMode,
    pub title: String,
    pub workspace_strategy: Workspace,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialMessage {
    /// T3 takes uploaded files only.
    pub attachments: [(); 0],
    pub message_id: String,
    pub text: String,
}

/// The project to create for `checkout`, named for its folder.
pub fn create_project(checkout: &Checkout, id: Uuid) -> ProjectMutation {
    let title = checkout.root.file_name().map_or_else(
        || "Project".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    ProjectMutation::Create {
        command_id: format!("sheprd:{id}:project"),
        project_id: id.to_string(),
        title,
        workspace_root: checkout.root.to_string_lossy().into_owned(),
    }
}

/// The model the thread runs: the one asked for, else the project's
/// default, else the first ready provider's default model.
pub fn model_for(
    request: &Request,
    project: Option<&Project>,
    providers: &[Provider],
) -> Result<ModelSelection, Error> {
    // The project's default, while it names an agent the server runs.
    let preferred = project
        .and_then(|project| project.default_model_selection.as_ref())
        .filter(|selection| {
            providers
                .iter()
                .any(|provider| provider.instance_id == selection.instance_id)
        });
    let provider = match (&request.provider, preferred) {
        (Some(id), _) => providers
            .iter()
            .find(|provider| &provider.instance_id == id)
            .ok_or_else(|| Error::UnknownAgent(id.clone()))?,
        (None, Some(selection)) => {
            if request.model.is_none() {
                return Ok(selection.clone());
            }
            providers
                .iter()
                .find(|provider| provider.instance_id == selection.instance_id)
                .ok_or(Error::NoAgent)?
        }
        (None, None) => providers.first().ok_or(Error::NoAgent)?,
    };
    let model = match &request.model {
        Some(slug) => provider
            .models
            .iter()
            .find(|model| &model.slug == slug)
            .ok_or_else(|| Error::UnknownAgent(format!("{}/{slug}", provider.instance_id)))?,
        None => provider
            .models
            .iter()
            .find(|model| model.is_default)
            .or_else(|| provider.models.first())
            .ok_or(Error::NoAgent)?,
    };
    Ok(ModelSelection {
        instance_id: provider.instance_id.clone(),
        model: model.slug.clone(),
    })
}

/// The launch of `request` in `checkout`'s project `project_id`, with ids
/// from `id`, so the same `id` asks for the same thread again.
pub fn launch(
    request: &Request,
    checkout: &Checkout,
    project_id: &str,
    model_selection: ModelSelection,
    id: Uuid,
) -> Launch {
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let title = request
        .title
        .clone()
        .or_else(|| {
            prompt
                .and_then(|text| text.lines().next())
                .map(|line| line.chars().take(TITLE_CHARS).collect())
        })
        .unwrap_or_else(|| "New thread".to_owned());
    let workspace_strategy = match (&request.worktree, &checkout.worktree) {
        (Some(new), _) => Workspace::Worktree {
            base_ref: new.base.clone(),
            branch: new.branch.clone(),
        },
        (None, Some(path)) => Workspace::ExistingWorktree {
            worktree_path: path.to_string_lossy().into_owned(),
        },
        (None, None) => Workspace::Root,
    };
    Launch {
        command_id: format!("sheprd:{id}"),
        creation_source: Source::Web,
        // Unless named, T3 names the thread from its first message, as its
        // own composer asks; until then the title is the prompt's first line.
        generate_title: request.title.is_none() && prompt.is_some(),
        initial_message: prompt.map(|text| InitialMessage {
            attachments: [],
            message_id: format!("sheprd:{id}:message"),
            text: text.to_owned(),
        }),
        interaction_mode: request.interaction_mode,
        model_selection,
        project_id: project_id.to_owned(),
        runtime_mode: request.runtime_mode,
        title,
        workspace_strategy,
    }
}

/// Starts `request` on the server behind `connection`, finding or creating
/// its project; returns the new thread's id.
pub fn start<S: Read + Write>(
    connection: &mut Connection<S>,
    request: &Request,
    id: Uuid,
) -> crate::Result<String> {
    let checkout = checkout(&request.cwd)?;
    let providers = steer::providers(&connection.call("server.getConfig", json!({}))?);
    let shell = shell(connection)?;
    let existing = project_for(&shell, &checkout.root);
    let model_selection = model_for(request, existing, &providers)?;
    let project_id = match existing {
        Some(project) => project.id.clone(),
        None => {
            match connection.call(
                MUTATE_PROJECT,
                serde_json::to_value(create_project(&checkout, id))?,
            ) {
                Ok(created) => created
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(Error::Answer(MUTATE_PROJECT))?
                    .to_owned(),
                // Another client may have created the root's project since
                // the list was read; T3 refuses a second one, so use it.
                Err(refused @ crate::Error::Call { .. }) => {
                    let now = self::shell(connection)?;
                    project_for(&now, &checkout.root)
                        .map(|project| project.id.clone())
                        .ok_or(refused)?
                }
                Err(error) => return Err(error),
            }
        }
    };
    let launch = launch(request, &checkout, &project_id, model_selection, id);
    let launched = connection.call(LAUNCH, serde_json::to_value(&launch)?)?;
    Ok(launched
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or(Error::Answer(LAUNCH))?
        .to_owned())
}

/// The thread list's first snapshot, after which the stream is stopped.
fn shell<S: Read + Write>(connection: &mut Connection<S>) -> crate::Result<Shell> {
    let request = connection.request("orchestration.subscribeShell", json!({}))?;
    let mut shell = Shell::default();
    loop {
        match connection.next_frame()? {
            ServerFrame::Chunk { request_id, values } if request_id == request => {
                for value in values {
                    shell.apply(value)?;
                }
                connection.interrupt(&request)?;
                return Ok(shell);
            }
            ServerFrame::Exit { request_id, exit } if request_id == request => {
                return Err(Error::Shell(exit.cause().unwrap_or_default()).into());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
