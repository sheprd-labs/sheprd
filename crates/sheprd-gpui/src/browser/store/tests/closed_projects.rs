use super::*;
use crate::browser::store::closed_projects::{
    MAX_CLOSED_PROJECT_FILE_BYTES, MAX_CLOSED_PROJECTS, Saved as SavedClosedProjects,
};
use sheprd_session::{Keeper, Workspace, Worktree};

fn workspace(cwd: &str, label: &str) -> Workspace {
    Workspace {
        ahead_behind: Some((2, 1)),
        branch: Some("feature/persist-sidebar-row".into()),
        cwd: cwd.into(),
        custom_label: true,
        id: "w_1".into(),
        keeper: Keeper::Herdr1,
        label: label.into(),
        status: sheprd_client::protocol::AgentStatus::Working,
        tabs: Vec::new(),
        tokens: Vec::new(),
        worktree: Some(Worktree {
            key: "/work/repo/.git".into(),
            label: "repo".into(),
            linked: true,
        }),
    }
}

#[test]
fn closed_projects_are_scoped_deduplicated_and_persisted() {
    let scope = Scope::local("/tmp/herdr.sock".as_ref());
    let other = Scope::endpoint("ssh:box");
    let mut store = Store::default();
    store.remember_closed_project(&scope, &workspace("/work/Chrome/project", "Project"));
    let first = store.closed_projects(&scope).next().unwrap().clone();
    assert!(store.closed_project(&scope, &first.id).is_some());
    assert!(store.closed_project(&other, &first.id).is_none());

    store.remember_closed_project(&scope, &workspace("/work/Chrome/project", "Renamed"));
    assert_eq!(store.closed_projects(&scope).count(), 1);
    assert_eq!(
        store.closed_project(&scope, &first.id).unwrap().label,
        "Renamed"
    );

    let bytes = serde_json::to_vec(&SavedClosedProjects {
        projects: store.closed_projects.clone(),
    })
    .unwrap();
    let restored: SavedClosedProjects = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.projects, store.closed_projects);
    let displayed = restored.projects[0].workspace();
    assert_eq!(displayed.cwd, "/work/Chrome/project");
    assert_eq!(
        displayed.branch.as_deref(),
        Some("feature/persist-sidebar-row")
    );
    assert_eq!(displayed.worktree.unwrap().key, "/work/repo/.git");
    assert_eq!(displayed.status, sheprd_client::protocol::AgentStatus::Idle);
}

#[test]
fn invalid_closed_projects_are_ignored() {
    let saved = r#"{"projects":[
        {"id":"bad","scope":"local","cwd":"/work/project","label":"project"},
        {"id":"closed-project-1","scope":"local","cwd":"relative","label":"project"},
        {"id":"closed-project-2","scope":"local","cwd":"/work/project","label":"project"}
    ]}"#;
    let mut parsed: SavedClosedProjects = serde_json::from_str(saved).unwrap();
    parsed.projects.retain(ClosedProject::valid);
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.projects[0].id, "closed-project-2");
}

#[test]
fn the_largest_valid_closed_project_list_fits_the_read_limit() {
    let scope = "\\".repeat(1024);
    let temp = std::env::temp_dir().display().to_string();
    let cwd = format!(
        "{temp}{}",
        "\\".repeat(sheprd_session::saved::MAX_CWD_BYTES - temp.len())
    );
    let label = "\\".repeat(sheprd_session::saved::MAX_LABEL_CHARS);
    let branch = "\\".repeat(1024);
    let key = "\\".repeat(8192);
    let projects = (0..MAX_CLOSED_PROJECTS)
        .map(|index| ClosedProject {
            id: format!("closed-project-{index}"),
            scope: scope.clone(),
            cwd: cwd.clone(),
            label: label.clone(),
            custom_label: true,
            branch: Some(branch.clone()),
            worktree: Some(ClosedProjectWorktree {
                key: key.clone(),
                label: label.clone(),
                linked: true,
            }),
        })
        .collect();
    let bytes = serde_json::to_vec(&SavedClosedProjects { projects }).unwrap();
    assert!(bytes.len() as u64 <= MAX_CLOSED_PROJECT_FILE_BYTES);
}

#[test]
fn closed_projects_survive_store_reload_in_their_own_state_file() {
    let directory = tempfile::tempdir().unwrap();
    let tabs_path = directory.path().join("browser-tabs.json");
    let scope = Scope::local("/tmp/herdr.sock".as_ref());
    let mut store = Store::load_at(Some(tabs_path.clone()), std::time::Duration::ZERO).unwrap();
    store.remember_closed_project(&scope, &workspace("/work/Chrome/project", "Project"));
    store.closed_project_writer.take().unwrap().finish();
    store.writer.take().unwrap().finish();
    drop(store);

    let restored = Store::load_at(Some(tabs_path), std::time::Duration::ZERO).unwrap();
    assert_eq!(restored.closed_projects(&scope).count(), 1);
    assert_eq!(
        restored.closed_projects(&scope).next().unwrap().cwd,
        "/work/Chrome/project"
    );
}
