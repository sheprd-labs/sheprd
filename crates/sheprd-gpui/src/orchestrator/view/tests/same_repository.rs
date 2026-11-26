//! Folders of one repository whose Git common directory was not read in
//! time are each a checkout, keyed by folder; the projects menu, header,
//! and picks still treat them as the one project their workers found.
use super::*;
use crate::orchestrator::{
    Repository, Request, Timing,
    repo::RepoInfo,
    view::{OrchestratorView, RepoSpec, projects::Entry, repo::Repo, rows::Tab},
};
use std::{collections::HashSet, sync::Arc};

fn info(common: &str) -> RepoInfo {
    RepoInfo {
        repository: Repository::Local(common.into()),
        main_root: common.trim_end_matches("/.git").into(),
        beads: false,
        beads_root: None,
        remote: None,
    }
}

/// The view with folders "apps", "packages", and "other" beside its own
/// checkout, each listing one issue, none found yet.
fn folders(view: &mut OrchestratorView, dir: &std::path::Path) {
    for folder in ["apps", "packages", "other"] {
        let request = Request {
            target: sheprd_client::ConnectTarget::Local,
            checkout: dir.join(folder).to_string_lossy().into_owned(),
            workspace_id: folder.into(),
            token: None,
            data_root: Some(dir.join(format!("{folder}-data"))),
            azure_store: Default::default(),
            gitlab_store: Default::default(),
            timing: Timing::default(),
        };
        view.repos.push(Repo::start(RepoSpec {
            key: folder.into(),
            host: "Local".into(),
            request,
        }));
    }
    for (index, repo) in view.repos.iter_mut().enumerate() {
        let issue = item(
            Provider::Github,
            &(index + 1).to_string(),
            "Issue",
            "2026-10-04T00:00:00Z",
        );
        repo.snapshot.items = Arc::new(vec![issue]);
    }
    view.tab = Tab::Issues;
}

/// Their workers find the first three are folders of /code/hub; "other"
/// is not.
fn found(view: &mut OrchestratorView) {
    for (index, repo) in view.repos.iter_mut().enumerate() {
        let common = if index < 3 {
            "/code/hub/.git"
        } else {
            "/code/other/.git"
        };
        repo.snapshot.repo = Some(info(common));
    }
    view.refresh_rows();
}

#[gpui::test]
fn folders_of_one_repository_are_one_project(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            folders(view, dir.path());
            found(view);
            let first = view.repos[0].key.clone();

            let entries = view.project_entries();
            let keys: Vec<_> = entries
                .iter()
                .map(|entry| match entry {
                    Entry::Repo { key, .. } => key.as_str(),
                    other => panic!("{other:?}"),
                })
                .collect();
            assert_eq!(keys, [first.as_str(), "other"]);
            assert_eq!(view.projects_listing().count(), 2);
            for folder in ["apps", "packages"] {
                assert_eq!(view.pick_of(folder), Some(first.clone()));
            }

            // Picking the repository lists every folder's items.
            view.projects = HashSet::from([first.clone()]);
            view.refresh_rows();
            assert_eq!(view.keys().len(), 3);
        })
    });
}

#[gpui::test]
fn a_folder_picked_before_its_repository_is_found_stays_listed(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            folders(view, dir.path());
            view.refresh_rows();
            // As opening the "packages" workspace's issues picks it.
            view.only_project("packages".into(), cx);
            assert_eq!(view.keys().len(), 1);

            found(view);
            let first = view.repos[0].key.clone();
            assert_eq!(view.projects, HashSet::from([first]));
            assert_eq!(view.keys().len(), 3);
        })
    });
}
