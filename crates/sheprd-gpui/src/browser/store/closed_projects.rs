//! Herdr 1 removes a workspace when its last tab closes. Keep the folder and
//! enough Git identity to show it in Spaces after it closes again.

use super::{ClosedProject, ClosedProjectWorktree, Scope, Store};
use crate::state_file;
use serde::{Deserialize, Serialize};
use sheprd_session::Workspace;
use std::path::Path;

pub(super) const MAX_CLOSED_PROJECTS: usize = 128;
pub(super) const MAX_CLOSED_PROJECT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const PREFIX: &str = "closed-project-";

#[derive(Deserialize, Serialize)]
pub(super) struct Saved {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) projects: Vec<ClosedProject>,
}

pub(super) fn load(
    tabs_path: Option<&Path>,
    wait: std::time::Duration,
) -> (
    Vec<ClosedProject>,
    Option<state_file::Writer<Saved>>,
    Option<std::fs::File>,
) {
    let Some(path) = tabs_path.map(|path| path.with_file_name("closed-projects.json")) else {
        return (Vec::new(), None, None);
    };
    let owner = match super::own(&path, wait) {
        Ok(owner) => owner,
        Err(error) => {
            tracing::warn!(%error, "Cannot own closed projects");
            return (Vec::new(), None, None);
        }
    };
    let projects = state_file::read(&path, MAX_CLOSED_PROJECT_FILE_BYTES)
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
        .map(|mut saved| {
            saved.projects.retain(ClosedProject::valid);
            saved.projects.truncate(MAX_CLOSED_PROJECTS);
            saved.projects
        })
        .unwrap_or_default();
    let writer = state_file::Writer::start("closed-projects", path).ok();
    (projects, writer, Some(owner))
}

impl Store {
    /// Projects retained for a daemon, in the order their final tabs closed.
    pub(crate) fn closed_projects<'a>(
        &'a self,
        scope: &'a Scope,
    ) -> impl Iterator<Item = &'a ClosedProject> + 'a {
        self.closed_projects
            .iter()
            .filter(move |project| project.scope == scope.as_str())
    }

    /// A retained project by its synthetic sidebar ID.
    pub(crate) fn closed_project(&self, scope: &Scope, id: &str) -> Option<&ClosedProject> {
        self.closed_projects
            .iter()
            .find(|project| project.scope == scope.as_str() && project.id == id)
    }

    /// Retains a local Herdr workspace after its last terminal tab closes.
    pub(crate) fn remember_closed_project(&mut self, scope: &Scope, workspace: &Workspace) {
        if self
            .suppressed
            .remove(&(scope.as_str().to_owned(), workspace.cwd.clone()))
        {
            return;
        }
        if workspace.cwd.is_empty()
            || workspace.cwd.len() > sheprd_session::saved::MAX_CWD_BYTES
            || !Path::new(&workspace.cwd).is_absolute()
        {
            return;
        }
        let existing = self
            .closed_projects
            .iter()
            .position(|project| project.scope == scope.as_str() && project.cwd == workspace.cwd);
        let id = existing.map_or_else(
            || self.next_closed_project_id(),
            |at| self.closed_projects[at].id.clone(),
        );
        let closed = ClosedProject {
            id,
            scope: scope.as_str().to_owned(),
            cwd: workspace.cwd.clone(),
            label: workspace.label.clone(),
            custom_label: workspace.custom_label,
            branch: workspace.branch.clone(),
            worktree: workspace
                .worktree
                .as_ref()
                .map(|tree| ClosedProjectWorktree {
                    key: tree.key.clone(),
                    label: tree.label.clone(),
                    linked: tree.linked,
                }),
        };
        if !closed.valid() {
            return;
        }
        if let Some(at) = existing {
            self.closed_projects.remove(at);
        } else if self.closed_projects.len() >= MAX_CLOSED_PROJECTS {
            self.closed_projects.remove(0);
        }
        self.closed_projects.push(closed);
        self.persist_closed_projects();
    }

    /// Forgets a retained project by its sidebar id, so its row leaves.
    pub(crate) fn forget_closed_project(&mut self, scope: &Scope, id: &str) -> bool {
        let Some(at) = self
            .closed_projects
            .iter()
            .position(|project| project.scope == scope.as_str() && project.id == id)
        else {
            return false;
        };
        self.closed_projects.remove(at);
        self.persist_closed_projects();
        true
    }

    /// Forgets the retained project of a folder and keeps the close in
    /// flight from remembering it again: an explicit close leaves no row.
    pub(crate) fn forget_closed_project_path(&mut self, scope: &Scope, cwd: &str) {
        let before = self.closed_projects.len();
        self.closed_projects
            .retain(|project| project.scope != scope.as_str() || project.cwd != cwd);
        self.suppressed
            .insert((scope.as_str().to_owned(), cwd.to_owned()));
        if self.closed_projects.len() != before {
            self.persist_closed_projects();
        }
    }

    fn persist_closed_projects(&self) {
        if !self.quitting
            && let Some(writer) = &self.closed_project_writer
        {
            writer.save(Saved {
                projects: self.closed_projects.clone(),
            });
        }
    }

    fn next_closed_project_id(&self) -> String {
        let next = self
            .closed_projects
            .iter()
            .filter_map(|project| project.id.strip_prefix(PREFIX)?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        format!("{PREFIX}{next}")
    }
}
