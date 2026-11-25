//! Which Herdr 1 workspace a phone's new worktree grows from.

use super::{Checkout, Source, Worktree, herdr_1_repository};
use crate::hub::{Layout, Workspace};
use sheprd_client::protocol::ClientShellWorktree;
use std::path::{Path, PathBuf};

const KEY: &str = "/code/app/.git";

fn workspace(id: &str, cwd: &str, linked: Option<bool>) -> Workspace {
    Workspace {
        branch: None,
        chats: Vec::new(),
        cwd: cwd.to_owned(),
        id: id.to_owned(),
        label: id.to_owned(),
        session: None,
        terminals: Vec::new(),
        worktree: linked.map(|linked| ClientShellWorktree {
            key: KEY.to_owned(),
            label: "app".to_owned(),
            is_linked_worktree: linked,
        }),
    }
}

fn layout(workspaces: Vec<Workspace>) -> Layout {
    Layout {
        workspaces,
        ..Layout::default()
    }
}

fn kept_none(_: &str) -> bool {
    false
}

/// The repository's own checkout at `/code/app`.
fn main_checkout() -> Checkout {
    Checkout {
        common_dir: PathBuf::from(KEY),
        linked: false,
        branch: Some("refs/heads/main".to_owned()),
    }
}

/// A linked worktree of `/code/app` on `branch`.
fn linked_checkout(branch: Option<&str>) -> Checkout {
    Checkout {
        common_dir: PathBuf::from(KEY),
        linked: true,
        branch: branch.map(|branch| format!("refs/heads/{branch}")),
    }
}

/// The workspace a new worktree of `checkout` grows from, without a kept one.
fn repository(layout: &Layout, checkout: &Checkout, session: Option<&str>) -> Option<String> {
    herdr_1_repository(layout, checkout, session, kept_none).map(|source| source.workspace)
}

#[test]
fn the_repository_checkout_grows_the_worktree_wherever_its_shell_is() {
    let layout = layout(vec![
        workspace("w2", "/code/app", Some(true)),
        workspace("w1", "/code/app/crates", Some(false)),
    ]);
    assert_eq!(
        herdr_1_repository(&layout, &main_checkout(), None, kept_none),
        Some(Source {
            workspace: "w1".to_owned(),
            base: None,
        }),
        "the repository's own checkout, not a linked worktree in its folder"
    );
}

#[test]
fn a_workspace_whose_shell_moved_to_another_repository_is_not_that_repository() {
    // `w1` is `/code/app`'s, though its shell is now in `/code/other`.
    let layout = layout(vec![workspace("w1", "/code/other", Some(false))]);
    let other = Checkout {
        common_dir: PathBuf::from("/code/other/.git"),
        ..main_checkout()
    };
    assert_eq!(repository(&layout, &other, None), None);
}

#[test]
fn a_plain_folder_and_a_kept_workspace_do_not_claim_the_repository() {
    let layout = layout(vec![
        workspace("w1", "/code/other", None),
        workspace("core:terminals", "/code/app", None),
        workspace("space-3", "/code/app", Some(false)),
        workspace("herdr:dev:w4", "/code/app", Some(false)),
    ]);
    let kept = |id: &str| id == "space-3";
    assert_eq!(
        herdr_1_repository(&layout, &main_checkout(), None, kept),
        None
    );
}

#[test]
fn a_checkout_before_its_first_worktree_uses_the_existing_workspace() {
    let workspaces = layout(vec![workspace("w1", "/code/app", None)]);
    assert_eq!(
        repository(&workspaces, &main_checkout(), None).as_deref(),
        Some("w1")
    );
    let indexed = layout(vec![
        workspace("w1", "/code/app", None),
        workspace("w2", "/code/app", Some(false)),
    ]);
    assert_eq!(
        repository(&indexed, &main_checkout(), None).as_deref(),
        Some("w2")
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_to_the_checkout_matches_before_worktree_metadata_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    std::fs::create_dir_all(app.join(".git")).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&app, &alias).unwrap();
    let checkout = Checkout {
        common_dir: std::fs::canonicalize(app.join(".git")).unwrap(),
        ..main_checkout()
    };
    let workspaces = layout(vec![workspace("w1", alias.to_str().unwrap(), None)]);
    assert_eq!(
        repository(&workspaces, &checkout, None).as_deref(),
        Some("w1")
    );
}

#[test]
fn a_repository_is_looked_for_in_the_session_asked_for() {
    let layout = layout(vec![
        workspace("w1", "/code/app", Some(false)),
        workspace("herdr:dev:w1", "/code/app", Some(false)),
    ]);
    let checkout = main_checkout();
    assert_eq!(
        repository(&layout, &checkout, Some("dev")).as_deref(),
        Some("herdr:dev:w1")
    );
    assert_eq!(repository(&layout, &checkout, None).as_deref(), Some("w1"));
    assert_eq!(repository(&layout, &checkout, Some("other")), None);
}

#[test]
fn a_title_names_the_herdr_1_worktree_workspace() {
    let worktree = Worktree {
        base: None,
        branch: "topic",
        repository: "w1",
        title: Some("Review"),
    };
    let params = worktree.params();
    assert_eq!(params["workspace_id"], "w1");
    assert_eq!(params["branch"], "topic");
    assert_eq!(params["focus"], false);
    assert_eq!(params["label"], "Review");
    let untitled = Worktree {
        title: None,
        ..worktree
    };
    assert!(untitled.params().get("label").is_none());
    assert!(untitled.params().get("base").is_none());
    let from_branch = Worktree {
        base: Some("refs/heads/feature"),
        ..worktree
    };
    assert_eq!(from_branch.params()["base"], "refs/heads/feature");
}

#[test]
fn a_linked_checkout_grows_from_its_main_checkout_at_its_branch() {
    let layout = layout(vec![
        workspace("w1", "/code/app", Some(false)),
        workspace("w2", "/code/worktrees/feature", Some(true)),
    ]);
    assert_eq!(
        herdr_1_repository(&layout, &linked_checkout(Some("feature")), None, kept_none),
        Some(Source {
            workspace: "w1".to_owned(),
            base: Some("refs/heads/feature".to_owned()),
        })
    );
}

#[test]
fn a_linked_checkout_without_its_main_checkout_or_branch_stays_on_the_core() {
    let alone = layout(vec![workspace("w2", "/code/worktrees/feature", Some(true))]);
    assert_eq!(
        repository(&alone, &linked_checkout(Some("feature")), None),
        None
    );
    let both = layout(vec![
        workspace("w1", "/code/app", Some(false)),
        workspace("w2", "/code/worktrees/feature", Some(true)),
    ]);
    assert_eq!(repository(&both, &linked_checkout(None), None), None);
}

#[test]
fn git_says_which_checkout_a_folder_is() {
    let same = |path: &Path| path.to_owned();
    assert_eq!(
        Checkout::parse(
            "/code/app/.git\n/code/app/.git\n",
            Some("refs/heads/main\n"),
            same
        ),
        Some(main_checkout())
    );
    assert_eq!(
        Checkout::parse(
            "/code/app/.git\n/code/app/.git/worktrees/feature\n",
            None,
            same
        ),
        Some(linked_checkout(None)),
        "a detached linked worktree has no branch"
    );
    assert_eq!(Checkout::parse("", None, same), None);
}

#[test]
fn a_folder_on_disk_is_read_through_git() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    std::fs::create_dir(&app).unwrap();
    let git = |cwd: &Path, args: &[&str]| {
        let status = sheprd_client::new_command("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&app, &["init", "--quiet", "--initial-branch=main"]);
    git(&app, &["commit", "--quiet", "--allow-empty", "-m", "start"]);
    git(
        &app,
        &["worktree", "add", "--quiet", "-b", "feature", "../feature"],
    );
    let key = std::fs::canonicalize(app.join(".git")).unwrap();
    let main = Checkout::of(&app.join(".git").join("..")).unwrap();
    assert_eq!(main.common_dir, key);
    assert!(!main.linked);
    let feature = Checkout::of(&dir.path().join("feature")).unwrap();
    assert_eq!(feature.common_dir, key);
    assert!(feature.linked);
    assert_eq!(feature.branch.as_deref(), Some("refs/heads/feature"));
    assert_eq!(Checkout::of(dir.path()), None, "not a checkout");
}
