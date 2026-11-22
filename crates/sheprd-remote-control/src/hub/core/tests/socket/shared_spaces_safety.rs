//! A closed Sheprd's changes to the shared document that must not lose
//! work: a recovered chat's pane, a tab another client adds while its
//! workspace closes, a document this version may not write, and a
//! worktree whose workspace is one of its folders.

use super::shared_spaces::{SCOPE, document, fake_sharing, shared};
use super::*;
use crate::control::layout::Op;
use sheprd_backend::TerminalId;
use sheprd_session::spaces::DOC;
use std::{collections::BTreeMap, path::Path};

fn sent(fake: &Fake) -> Option<Value> {
    let bytes = fake.inputs.recv_timeout(Duration::from_millis(500)).ok()?;
    Some(serde_json::from_slice(&bytes).unwrap())
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

#[test]
fn a_recovered_chats_pane_is_changed_by_its_replacements_id() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(
        dir.path(),
        Some(document()),
        vec![record("t1"), record("t7")],
    );
    source.poll().unwrap();
    lock(&source.poller).replacements = BTreeMap::from([(
        TerminalId::new("t1").unwrap(),
        TerminalId::new("t7").unwrap(),
    )]);
    let tab = TabRef::Doc("tab-a".into());
    source
        .arrange_saved(&tab, "t7", &Op::RenamePane(Some("server".into())))
        .unwrap();
    source.arrange_saved(&tab, "t7", &Op::Zoom).unwrap();
    // Only the fields changed: the document keeps naming the pane by the
    // terminal it held, and readers show the replacement in its place
    // (`kept::remap`).
    let spaces = shared(&fake);
    let kept = spaces.tab("tab-a").unwrap();
    assert_eq!(kept.panes.terminals()[0].as_str(), "t1");
    assert_eq!(kept.names.get("t1").map(String::as_str), Some("server"));
    assert_eq!(kept.zoomed.as_ref().map(TerminalId::as_str), Some("t1"));
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_tab_added_while_its_workspace_closes_has_its_terminals_ended_too() {
    let dir = tempfile::tempdir().unwrap();
    let records = vec![record("t1"), record("t3")];
    let (source, fake) = fake_sharing(dir.path(), Some(document()), records);
    source.poll().unwrap();
    let mut added = document();
    added["tabs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "tab-b", "workspace_id": "space-a", "title": "b", "panes": "t3"}));
    *lock(&fake.first) = Some(added);
    let close = Op::CloseWorkspace {
        workspace: "space-a".into(),
    };
    source.arrange_saved(&TabRef::File(0), "", &close).unwrap();
    let killed: Vec<Value> = std::iter::from_fn(|| sent(&fake))
        .map(|request| request["params"]["terminal"].clone())
        .collect();
    assert_eq!(killed, [json!("t1"), json!("t3")]);
    let spaces = shared(&fake);
    assert!(spaces.workspace("space-a").is_none());
    assert!(spaces.tabs().is_empty());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_newer_document_refuses_a_close_or_delete_before_anything_ends() {
    let dir = tempfile::tempdir().unwrap();
    let mut newer = document();
    newer["format"] = json!(2);
    let (source, fake) = fake_sharing(dir.path(), Some(newer.clone()), vec![record("t1")]);
    source.poll().unwrap();
    for op in [
        Op::CloseWorkspace {
            workspace: "space-a".into(),
        },
        Op::RemoveWorktree {
            workspace: "space-a".into(),
            discard: true,
        },
    ] {
        let error = source.arrange_saved(&TabRef::File(0), "", &op).unwrap_err();
        assert!(
            matches!(
                error,
                HubError::Core(Error::Client(sheprd_core_client::Error::Spaces(
                    sheprd_session::spaces::Error::Newer(2)
                )))
            ),
            "{error}"
        );
    }
    assert!(sent(&fake).is_none(), "no terminal ended");
    assert_eq!(lock(&fake.docs)[DOC], (newer, 1));
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_workspace_in_a_folder_of_a_worktree_deletes_the_whole_checkout() {
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
    let folder = checkout.join("src");
    std::fs::create_dir(&folder).unwrap();
    let mut doc = document();
    doc["workspaces"][0]["cwd"] = json!(folder.to_str().unwrap());
    let (source, fake) = fake_sharing(dir.path(), Some(doc), vec![record("t1")]);
    assert_eq!(source.scope.as_deref(), Some(SCOPE));
    source.poll().unwrap();
    let remove = Op::RemoveWorktree {
        workspace: "space-a".into(),
        discard: false,
    };
    source.arrange_saved(&TabRef::File(0), "", &remove).unwrap();
    assert!(!checkout.exists());
    assert!(shared(&fake).workspace("space-a").is_none());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_pane_already_closed_does_not_stop_its_workspace_closing() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = document();
    doc["tabs"][0]["panes"] = json!({"axis": "row", "first": "t1", "second": "t5"});
    // The core no longer lists `t5`: a phone closed it.
    let (source, fake) = fake_sharing(dir.path(), Some(doc), vec![record("t1")]);
    source.poll().unwrap();
    let close = Op::CloseWorkspace {
        workspace: "space-a".into(),
    };
    source.arrange_saved(&TabRef::File(0), "", &close).unwrap();
    let killed: Vec<Value> = std::iter::from_fn(|| sent(&fake))
        .map(|request| request["params"]["terminal"].clone())
        .collect();
    assert_eq!(killed, [json!("t1")]);
    assert!(shared(&fake).workspace("space-a").is_none());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_split_whose_write_landed_keeps_its_shell_though_the_answer_was_lost() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    fake.lose.store(true, Ordering::SeqCst);
    let tab = TabRef::Doc("tab-a".into());
    let split = source.arrange_saved(&tab, "t1", &Op::SplitRight).unwrap();
    assert_eq!(split.as_deref(), Some("t9"));
    assert_eq!(sent(&fake).unwrap()["method"], "terminal.run");
    assert!(sent(&fake).is_none(), "the shell was not ended");
    let spaces = shared(&fake);
    let held = spaces.tab("tab-a").unwrap().panes.terminals();
    assert_eq!(held.len(), 2);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_split_that_did_not_land_ends_its_shell() {
    let dir = tempfile::tempdir().unwrap();
    let (source, fake) = fake_sharing(dir.path(), Some(document()), vec![record("t1")]);
    source.poll().unwrap();
    let tab = TabRef::Doc("tab-gone".into());
    assert!(source.arrange_saved(&tab, "t1", &Op::SplitRight).is_err());
    assert_eq!(sent(&fake).unwrap()["method"], "terminal.run");
    let kill = sent(&fake).unwrap();
    assert_eq!(
        (kill["method"].as_str(), kill["params"]["terminal"].as_str()),
        (Some("terminal.kill"), Some("t9"))
    );
    drop(source);
    fake.server.join().unwrap();
}
