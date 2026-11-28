//! A phone's new worktree of a repository Herdr 1 shows is Herdr 1's, so it
//! joins that repository's group, and its chat starts in the shell Herdr 1
//! opens there rather than in a second tab on the core.

use super::*;
use crate::hub::{NewWorkspace, Workspace};
use sheprd_chat_protocol::{Agent, EndpointId, TOKEN};
use sheprd_client::protocol::ClientShellWorktree;
use std::path::{Path, PathBuf};

/// Herdr 1's API socket beside `dir/h-client.sock`, answering one request
/// per connection with `answers`' results in turn and handing the requests
/// back.
fn herdr_1(dir: &Path, answers: Vec<Value>) -> thread::JoinHandle<Vec<Value>> {
    let listener = UnixListener::bind(dir.join("h.sock")).unwrap();
    thread::spawn(move || {
        answers
            .into_iter()
            .map(|answer| {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let reply = json!({"id": request["id"], "result": answer});
                (&stream)
                    .write_all(format!("{reply}\n").as_bytes())
                    .unwrap();
                request
            })
            .collect()
    })
}

fn repository(id: &str, cwd: &str, key: &str) -> Workspace {
    Workspace {
        branch: Some("main".to_owned()),
        chats: Vec::new(),
        cwd: cwd.to_owned(),
        id: id.to_owned(),
        label: "app".to_owned(),
        session: None,
        terminals: Vec::new(),
        worktree: Some(ClientShellWorktree {
            key: key.to_owned(),
            label: "app".to_owned(),
            is_linked_worktree: false,
        }),
    }
}

fn chat_records() -> Vec<Value> {
    let mut chat = record("t9");
    chat["labels"] = json!({TOKEN: EndpointId::random().to_string()});
    chat["foreground"] = json!({"pid": 42, "name": "sheprd-acp", "argv": ["sheprd-acp"]});
    vec![record("t1"), chat]
}

fn run_request(fake: &Fake) -> Value {
    let bytes = fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn a_worktree_of_a_herdr_1_repository_is_made_there_with_its_chat_in_its_root_pane() {
    let dir = tempfile::Builder::new()
        .prefix("hw")
        .tempdir_in("/tmp")
        .unwrap();
    let checkout = "/code/worktrees/app/topic";
    let herdr = herdr_1(
        dir.path(),
        vec![
            json!({
                "type": "worktree_created",
                "workspace": {"workspace_id": "w9"},
                "root_pane": {"pane_id": "w9:p1"},
                "worktree": {"path": checkout, "branch": "topic"},
            }),
            json!({"type": "ok"}),
        ],
    );
    // A repository on disk, as Git says which one a folder is.
    let app = dir.path().join("app");
    std::fs::create_dir(&app).unwrap();
    let init = sheprd_client::new_command("git")
        .args(["init", "--quiet"])
        .current_dir(&app)
        .status()
        .unwrap();
    assert!(init.success());
    let app = std::fs::canonicalize(app).unwrap();
    let cwd = app.to_str().unwrap();
    let key = app.join(".git");
    let (source, fake) = fake_with_doc(false, Value::Null, chat_records());
    source.poll().unwrap();
    let layout = Layout {
        connected: true,
        workspaces: vec![repository("w1", cwd, key.to_str().unwrap())],
        ..Layout::default()
    };
    let mut hub = Hub::offline_in(layout, None, Some(dir.path().join("h-client.sock")));
    hub.core = Some(source.clone());
    hub.program = Some(PathBuf::from("/bin/sheprd-acp"));
    // Herdr 1 makes the checkout: none is made here.
    hub.worktrees = None;
    let created = hub
        .create_workspace(NewWorkspace {
            base: None,
            checkout: None,
            shell: false,
            agent: Agent::Claude,
            branch: Some("topic"),
            cwd,
            session: None,
            title: Some("Review"),
        })
        .await
        .unwrap();
    assert_eq!(created.workspace, "w9");
    // The shell Herdr 1 opened in the worktree runs the chat: no second tab.
    assert_eq!(created.pane, "w9:p1");
    let asked = herdr.join().unwrap();
    assert_eq!(asked[0]["method"], "worktree.create");
    let params = &asked[0]["params"];
    assert_eq!(params["workspace_id"], "w1");
    assert_eq!(params["branch"], "topic");
    assert_eq!(params["label"], "Review");
    assert_eq!(params["focus"], false);
    assert_eq!(asked[1]["method"], "pane.send_input");
    assert_eq!(asked[1]["params"]["pane_id"], "w9:p1");
    let line = asked[1]["params"]["text"].as_str().unwrap();
    assert!(line.contains("/bin/sheprd-acp"), "{line}");
    // Nothing started on the core.
    assert!(
        fake.inputs
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_new_core_workspace_carries_its_title_for_sheprd_to_keep() {
    let (source, fake) = fake_with_doc(false, Value::Null, chat_records());
    source.poll().unwrap();
    let mut hub = Hub::offline_in(Layout::default(), None, None);
    hub.core = Some(source.clone());
    hub.program = Some(PathBuf::from("/bin/sheprd-acp"));
    hub.create_workspace(NewWorkspace {
        base: None,
        checkout: None,
        shell: false,
        agent: Agent::Claude,
        branch: None,
        cwd: "/code/one",
        session: None,
        title: Some("Review"),
    })
    .await
    .unwrap();
    let params = &run_request(&fake)["params"];
    let labels = &params["labels"];
    assert_eq!(labels[sheprd_session::NEW_WORKSPACE_LABEL], "/code/one");
    assert_eq!(labels[sheprd_session::NEW_WORKSPACE_TITLE_LABEL], "Review");
    drop((hub, source));
    fake.server.join().unwrap();
}
