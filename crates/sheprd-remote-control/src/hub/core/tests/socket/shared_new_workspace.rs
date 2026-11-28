//! A phone's new workspace or worktree on the core, kept in the core's
//! shared document by the daemon once that was written, and left to a
//! desktop to adopt from its terminal's label until then.

use super::shared_spaces::{document, fake_sharing, shared};
use super::*;
use crate::hub::NewWorkspace;
use sheprd_chat_protocol::Agent;
use sheprd_session::{NEW_WORKSPACE_LABEL, spaces::DOC};
use std::path::{Path, PathBuf};

fn hub_on(source: &Arc<Source>) -> Hub {
    let mut hub = Hub::offline_in(Layout::default(), None, None);
    hub.core = Some(source.clone());
    hub.program = Some(PathBuf::from("/bin/sheprd-acp"));
    hub
}

fn started(fake: &Fake) -> Value {
    let bytes = fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn git(dir: &Path, args: &[&str]) -> bool {
    sheprd_client::new_command("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[tokio::test]
async fn a_phones_new_workspace_is_kept_in_the_document_with_its_first_tab() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let hub = hub_on(&source);
    let created = hub
        .create_workspace(NewWorkspace {
            base: None,
            checkout: None,
            shell: false,
            agent: Agent::Claude,
            branch: None,
            cwd: "/code/new",
            session: None,
            title: None,
        })
        .await
        .unwrap();
    let run = started(&fake);
    // Still labelled for desktops that do not read the document.
    assert_eq!(run["params"]["labels"][NEW_WORKSPACE_LABEL], "/code/new");
    let spaces = shared(&fake);
    let kept = spaces.workspace(&created.workspace).unwrap();
    assert!(kept.id.starts_with("space-") && kept.id.len() == "space-".len() + 16);
    assert_eq!(
        (kept.label.as_str(), kept.cwd.as_str()),
        ("new", "/code/new")
    );
    assert!(!kept.custom);
    let tab = spaces.tabs_of(&kept.id).next().unwrap();
    assert!(tab.id.starts_with("tab-"));
    assert_eq!(tab.title, Agent::Claude.display_name());
    assert_eq!(tab.panes.terminals()[0].as_str(), "t9");
    assert!(spaces.workspace("space-a").is_some(), "what was kept stays");
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_new_workspace_in_a_kept_folder_joins_that_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let hub = hub_on(&source);
    let created = hub
        .create_workspace(NewWorkspace {
            base: None,
            checkout: None,
            shell: false,
            agent: Agent::Claude,
            branch: None,
            cwd: "/code/app",
            session: None,
            title: None,
        })
        .await
        .unwrap();
    assert_eq!(created.workspace, "space-a");
    let spaces = shared(&fake);
    assert_eq!(spaces.workspaces().len(), 1);
    assert_eq!(spaces.tabs_of("space-a").count(), 2);
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_new_workspace_waits_for_sheprd_while_the_document_was_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), None, vec![record("t1")]);
    source.poll().unwrap();
    let hub = hub_on(&source);
    let created = hub
        .create_workspace(NewWorkspace {
            base: None,
            checkout: None,
            shell: false,
            agent: Agent::Claude,
            branch: None,
            cwd: "/code/new",
            session: None,
            title: None,
        })
        .await
        .unwrap();
    assert_eq!(created.workspace, new_workspace_id("/code/new"));
    assert!(
        !lock(&fake.docs).contains_key(DOC),
        "Sheprd adopts it and writes"
    );
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_phones_new_worktree_is_kept_in_its_new_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("project");
    std::fs::create_dir(&repository).unwrap();
    assert!(git(&repository, &["init", "--quiet"]));
    assert!(git(
        &repository,
        &["commit", "--quiet", "--allow-empty", "-m", "start"]
    ));
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let mut hub = hub_on(&source);
    let root = dir.path().join("worktrees");
    hub.worktrees = Some(root.clone());
    let created = hub
        .create_workspace(NewWorkspace {
            base: None,
            checkout: None,
            shell: false,
            agent: Agent::Claude,
            branch: Some("topic"),
            cwd: repository.to_str().unwrap(),
            session: None,
            title: None,
        })
        .await
        .unwrap();
    let checkout = root.join("project").join("topic");
    let spaces = shared(&fake);
    let kept = spaces.workspace(&created.workspace).unwrap();
    assert_eq!(kept.cwd, checkout.to_str().unwrap());
    assert_eq!(kept.label, "topic");
    drop((hub, source));
    fake.server.join().unwrap();
}

#[test]
fn deleting_a_kept_worktree_keeps_one_with_changes_until_the_phone_discards_them() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("project");
    std::fs::create_dir(&repository).unwrap();
    assert!(git(&repository, &["init", "--quiet"]));
    assert!(git(
        &repository,
        &["commit", "--quiet", "--allow-empty", "-m", "start"]
    ));
    let checkout = std::fs::canonicalize(dir.path()).unwrap().join("topic");
    let path = checkout.to_str().unwrap();
    assert!(git(
        &repository,
        &["worktree", "add", "--quiet", "-b", "topic", path]
    ));
    let mut doc = document();
    doc["workspaces"][0]["cwd"] = json!(path);
    let (source, fake) = fake_sharing(dir.path(), Some(doc), vec![record("t1")]);
    source.poll().unwrap();
    let remove = crate::control::layout::Op::RemoveWorktree {
        workspace: "space-a".into(),
        discard: false,
    };
    std::fs::write(checkout.join("work.txt"), "unsaved").unwrap();
    let error = source
        .arrange_saved(&TabRef::File(0), "", &remove)
        .unwrap_err();
    assert!(matches!(
        error,
        HubError::Core(Error::Arrange(super::super::arrange::Error::WorktreeKept))
    ));
    // The phone hears a conflict it may resolve by discarding the changes.
    let response = axum::response::IntoResponse::into_response(crate::web::ApiError::from(error));
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
    assert!(checkout.join("work.txt").exists());
    assert!(shared(&fake).workspace("space-a").is_some());

    // Confirmed on the phone: Git discards the change with the checkout.
    let discard = crate::control::layout::Op::RemoveWorktree {
        workspace: "space-a".into(),
        discard: true,
    };
    source
        .arrange_saved(&TabRef::File(0), "", &discard)
        .unwrap();
    assert!(!checkout.exists());
    let killed: Value =
        serde_json::from_slice(&fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    assert_eq!(killed["params"]["terminal"], "t1");
    assert!(shared(&fake).workspace("space-a").is_none());
    // The branch stays, as Git keeps it.
    assert!(git(
        &repository,
        &["rev-parse", "--verify", "--quiet", "topic"]
    ));
    drop(source);
    fake.server.join().unwrap();
}
