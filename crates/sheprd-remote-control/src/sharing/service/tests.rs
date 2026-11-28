use super::*;
use crate::sharing::gateway::peer::Peer;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sheprd_chat_protocol::{
    Cursor, Entry, EntryId, EntryKind, SessionInfo, Tool, ToolContent, ToolKind, ToolStatus,
    Transcript,
};
use sheprd_relay::client;
use std::time::Duration;

fn sample_transcript() -> Transcript {
    Transcript {
        agent: "Codex".into(),
        session: Some(SessionInfo {
            can_steer: false,
            id: "session-phase2".into(),
            cwd: Some("/private/worktree".into()),
            title: Some("Private title".into()),
            continued: None,
        }),
        entries: vec![
            Entry {
                id: EntryId(1),
                kind: EntryKind::User {
                    guest: None,
                    text: "Can you fix the reconnect bug?".into(),
                    files: 0,
                    note: None,
                },
            },
            Entry {
                id: EntryId(2),
                kind: EntryKind::Thought {
                    text: "Internal chain of thought".into(),
                },
            },
            Entry {
                id: EntryId(3),
                kind: EntryKind::Tool(Tool {
                    call_id: "call-secret".into(),
                    parent: None,
                    title: "Read stream.rs".into(),
                    kind: ToolKind::Read,
                    status: ToolStatus::Completed,
                    locations: vec!["/private/secret/stream.rs".into()],
                    content: vec![ToolContent::Text {
                        text: "Checked cursor guard".into(),
                    }],
                    started_ms: None,
                }),
            },
            Entry {
                id: EntryId(4),
                kind: EntryKind::Agent {
                    text: "Moved the cursor check ahead of the update.".into(),
                },
            },
        ],
        ..Transcript::default()
    }
}

#[test]
fn preview_enforces_from_now_and_tool_opt_in() {
    let transcript = sample_transcript();

    let from_now = project_preview(&transcript, ShareHistory::FromNow, false).unwrap();
    assert!(from_now.entries.is_empty());
    assert_eq!(from_now.total_entries, 0);

    let all_no_tools = project_preview(&transcript, ShareHistory::All, false).unwrap();
    assert_eq!(all_no_tools.total_entries, 2);
    assert_eq!(all_no_tools.entries[0].role, PreviewRole::User);
    assert_eq!(all_no_tools.entries[1].role, PreviewRole::Agent);
    assert!(!format!("{all_no_tools:?}").contains("Internal chain of thought"));
    assert!(!format!("{all_no_tools:?}").contains("Read stream.rs"));

    let all_with_tools = project_preview(&transcript, ShareHistory::All, true).unwrap();
    assert_eq!(all_with_tools.total_entries, 3);
    assert_eq!(all_with_tools.entries[1].role, PreviewRole::Tool);
    assert!(all_with_tools.entries[1].text.contains("Read stream.rs"));
    assert!(!format!("{all_with_tools:?}").contains("/private/secret"));
    assert!(!format!("{all_with_tools:?}").contains("call-secret"));
}

#[test]
fn share_link_debug_is_redacted() {
    let link = ShareLink::new("https://relay.example/share/#super-secret-fragment".into());
    assert_eq!(format!("{link:?}"), "[redacted share link]");
    assert_eq!(
        link.expose(),
        "https://relay.example/share/#super-secret-fragment"
    );
}

#[tokio::test]
async fn service_creates_lists_streams_and_revokes_over_real_loopback_relay() {
    let (endpoint, info) =
        sheprd_relay::bind_quic("127.0.0.1:0".parse().unwrap(), None, None).unwrap();
    let quic = endpoint.local_addr().unwrap();
    let pin = URL_SAFE_NO_PAD
        .decode(&info.pin)
        .unwrap()
        .try_into()
        .unwrap();
    let (_, hub) = sheprd_relay::app(&sheprd_relay::Options::default(), Some(info)).unwrap();
    tokio::spawn(sheprd_relay::quic::run(endpoint, hub));

    let dir = tempfile::tempdir().unwrap();
    let sharing_dir = dir.path().join("sharing");
    let service = tokio::task::spawn_blocking(move || Service::open(&sharing_dir))
        .await
        .unwrap()
        .unwrap();
    assert!(service.list().is_empty());

    let endpoint_id = EndpointId::random();
    let cursor = Cursor::new_epoch();
    let initial = sample_transcript();
    let (source_tx, feed) = watch::channel(Some(Snapshot {
        endpoint: endpoint_id.clone(),
        cursor,
        transcript: Arc::new(initial.clone()),
    }));
    let feed_task = tokio::spawn(std::future::pending::<()>());
    let relay = Endpoint::parse("https://relay.example.test").unwrap();
    let host_conn = client::connect(quic, pin).await.unwrap();
    let guest_conn = client::connect(quic, pin).await.unwrap();

    let created = service
        .create_on(
            "w1:p1",
            feed,
            feed_task,
            &relay,
            host_conn,
            &[],
            ShareHistory::All,
            false,
            Duration::from_secs(600),
        )
        .await
        .unwrap();

    assert_eq!(created.share.pane, "w1:p1");
    assert_eq!(created.share.grants, 1);
    assert_eq!(created.share.relay, "https://relay.example.test");
    assert_eq!(service.list().len(), 1);

    let (invite, offer) = link::decode(created.link.expose(), &relay, now_ms()).unwrap();
    let mut peer = Peer::connect(
        &guest_conn,
        &offer,
        invite.share,
        invite.grant,
        &invite.capability,
    )
    .await
    .unwrap();
    let first = peer.read(None).await.unwrap();
    let Update::Snapshot {
        cursor: pub_cursor,
        transcript,
    } = first
    else {
        panic!("expected snapshot, got {first:?}");
    };
    assert_eq!(transcript.entries.len(), 2);

    // Push a live update and verify the guest sees only allowed content.
    let mut updated = initial;
    updated.entries.push(Entry {
        id: EntryId(5),
        kind: EntryKind::Agent {
            text: "Regression test passes.".into(),
        },
    });
    source_tx
        .send(Some(Snapshot {
            endpoint: endpoint_id,
            cursor: Cursor {
                seq: cursor.seq + 1,
                ..cursor
            },
            transcript: Arc::new(updated),
        }))
        .unwrap();
    let delta = peer.read(Some(pub_cursor)).await.unwrap();
    assert!(matches!(delta, Update::Changes { .. }));

    // Revoking the share closes the live stream and clears the list.
    service
        .revoke(ShareId(created.share.share), None)
        .await
        .unwrap();
    assert!(service.list().is_empty());
    assert!(peer.read(Some(pub_cursor)).await.is_err());
}

mod browser;
