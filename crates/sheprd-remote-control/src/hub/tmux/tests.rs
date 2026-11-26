use super::*;
use serde_json::json;
use sheprd_backend::{Process, TerminalId};
use sheprd_chat_protocol::{EndpointId, TOKEN};

fn chat(n: u64) -> TerminalInfo {
    let id = TerminalId::new(format!("tmux-00000000-0000-4000-8000-{n:012x}")).unwrap();
    TerminalInfo {
        id,
        state: RunState::Running,
        agent: AgentStatus::Working,
        cwd: Some("/repo".into()),
        name: None,
        title: Some("Fix the build".into()),
        reporter: Some("Codex".into()),
        foreground: Some(Process {
            name: "sheprd-acp".into(),
            pid: 1,
            argv: vec![],
        }),
        labels: [(TOKEN.to_owned(), EndpointId::random().to_string())].into(),
    }
}

#[test]
fn discovers_each_live_helper_once_and_uses_its_saved_workspace_and_pane_name() {
    let info = chat(1);
    let saved = SavedTabs::parse(&serde_json::to_vec(&json!({
        "workspaces":[{"id":"kept", "scope":"s", "label":"Builds", "cwd":"/repo"}],
        "tabs":[{"id":7,"scope":"s","workspace_id":"kept","title":"Desktop tab",
            "location":{"kind":"core_terminal","panes":info.id.as_str(),"names":{(info.id.as_str()):"Release"}}}]
    })).unwrap()).unwrap();
    let mut shell = chat(2);
    shell.foreground.as_mut().unwrap().name = "zsh".into();
    let mut dead = chat(3);
    dead.state = RunState::Lost;
    let layout = project(
        vec![shell, dead, info.clone(), info.clone()],
        Some((&saved, "s")),
        &Default::default(),
    );
    assert!(layout.connected);
    assert_eq!(layout.working, 1);
    let key = crate::control::favorites::tmux_pane(info.id.as_str());
    let (workspace, row) = layout.chat(&key).unwrap();
    assert_eq!((&*workspace.id, &*workspace.label), ("kept", "Builds"));
    assert_eq!(row.title, "Release");
    assert_eq!(row.endpoint.as_str(), info.labels[TOKEN]);
    assert!(!crate::control::favorites::primary_pane(&key));
    assert_eq!(layout.chats().count(), 1);
}

#[test]
fn a_missing_or_unrelated_scope_never_claims_a_chat_and_the_inventory_is_bounded() {
    let layout = project((0..100).map(chat).collect(), None, &Default::default());
    assert_eq!(layout.workspaces[0].id, "tmux:chats");
    assert_eq!(layout.chats().count(), MAX_CHATS);
    assert!(!project(vec![], None, &Default::default()).connected);
}

#[test]
fn a_named_sessions_tab_joins_its_own_workspace() {
    let info = chat(42);
    let saved = SavedTabs::parse(
        &serde_json::to_vec(&json!({
            "tabs":[{"id":1,"scope":"side-socket","workspace_id":"w1","title":"Chat",
                "location":{"kind":"core_terminal","panes":info.id.as_str()}}]
        }))
        .unwrap(),
    )
    .unwrap();
    let scopes = [("side".into(), "side-socket".into())].into();
    let layout = project(vec![info], Some((&saved, "primary")), &scopes);
    assert_eq!(layout.workspaces[0].id, "herdr:side:w1");
}

#[test]
fn a_host_using_tmux_offers_a_home_to_start_its_first_chat() {
    let mut layout = project(vec![], None, &Default::default());
    list_home(&mut layout, None, Path::new("/home/guest"));
    list_home(&mut layout, None, Path::new("/home/guest"));
    assert!(layout.connected);
    assert_eq!(layout.workspaces.len(), 1);
    let home = layout.workspace(sheprd_session::DEFAULT_WORKSPACE).unwrap();
    assert_eq!(home.cwd, "/home/guest");
    assert!(home.chats.is_empty());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_client_subscribes_and_sends_a_command_without_either_herdr() {
    use sheprd_chat_protocol::{
        ClientMessage, Command, Cursor, ServerMessage, Transcript, read_frame, write_frame,
    };
    use sheprd_client::private_endpoint::Listener;
    let dir = tempfile::tempdir().unwrap();
    let layout = project(vec![chat(1)], None, &Default::default());
    let row = layout.chats().next().unwrap();
    let pane = row.pane.clone();
    let path = sheprd_chat_protocol::endpoint_path(dir.path(), &row.endpoint);
    let listener = Listener::bind(&path).unwrap();
    let epoch = uuid::Uuid::new_v4();
    let (opened, provider) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut reader, mut writer, _close) = listener.accept().unwrap().split().unwrap();
        assert!(matches!(
            read_frame::<_, ClientMessage>(&mut reader).unwrap(),
            Some(ClientMessage::Hello { .. })
        ));
        write_frame(
            &mut writer,
            &ServerMessage::Welcome {
                version: sheprd_chat_protocol::VERSION,
                agent: "Codex".into(),
                pane: None,
            },
        )
        .unwrap();
        write_frame(
            &mut writer,
            &ServerMessage::Snapshot {
                cursor: Cursor { epoch, seq: 0 },
                transcript: Box::new(Transcript::new("Codex")),
            },
        )
        .unwrap();
        provider.recv_timeout(Duration::from_secs(5)).unwrap();
        write_frame(
            &mut writer,
            &ServerMessage::Change {
                cursor: Cursor { epoch, seq: 1 },
                change: sheprd_chat_protocol::Change::Status {
                    status: sheprd_chat_protocol::Status::Idle,
                },
            },
        )
        .unwrap();
        let Some(ClientMessage::Command {
            id,
            command: Command::Cancel,
        }) = read_frame(&mut reader).unwrap()
        else {
            panic!("missing client command")
        };
        write_frame(&mut writer, &ServerMessage::Ack { id, rejected: None }).unwrap();
    });
    let hub = crate::hub::Hub::offline_with_chats(layout, dir.path().to_owned());
    let link = hub.chat(&pane).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !link.view().connected {
            tokio::time::sleep(Duration::from_millis(10)).await
        }
    })
    .await
    .unwrap();
    assert!(!hub.chat_ready(&pane, Duration::from_millis(25)).await);
    opened.send(()).unwrap();
    assert!(hub.chat_ready(&pane, Duration::from_secs(2)).await);
    assert_eq!(link.command(Command::Cancel).await.unwrap(), None);
    link.close();
    server.join().unwrap();
}
