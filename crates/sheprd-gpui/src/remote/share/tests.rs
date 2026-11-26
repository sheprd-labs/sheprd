use super::*;
use crate::{config::RemoteConfig, sidebar::layout_tests::fixture_window};
use core::prelude::v1::test;
use gpui::{Entity, TestAppContext, VisualTestContext};
use sheprd_remote_control::control::{
    self, RelayState, RelayStatus, Request, Response, Status,
    sharing::{
        PreviewEntry, PreviewRole, ShareCreated, ShareHistory, ShareLink, SharePreview,
        ShareSummary,
    },
};
use std::path::Path;
use uuid::Uuid;

fn fixture_status() -> Status {
    let mut relays = std::collections::BTreeMap::new();
    relays.insert(
        "https://relay.example.test".into(),
        RelayStatus::from(RelayState::Listening { rtt_ms: Some(12) }),
    );
    Status {
        boot_id: String::new(),
        address_details: Default::default(),
        version: "0.1.0".into(),
        port: 7443,
        addresses: vec!["https://127.0.0.1:7443".into()],
        fingerprint: "AA:BB".into(),
        computer: "Studio".into(),
        herdr: true,
        chats: 1,
        devices: 0,
        connected: 0,
        github_asks: 0,
        voice_asks: 0,
        relays,
    }
}

fn mock_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::ShareList => Ok(Response::Shares { shares: Vec::new() }),
        Request::SharePreview {
            history,
            include_tools,
            ..
        } => {
            let mut entries = Vec::new();
            if *history == ShareHistory::All {
                entries.push(PreviewEntry {
                    role: PreviewRole::User,
                    text: "Fix session reconnects".into(),
                });
                if *include_tools {
                    entries.push(PreviewEntry {
                        role: PreviewRole::Tool,
                        text: "Read stream.rs".into(),
                    });
                }
                entries.push(PreviewEntry {
                    role: PreviewRole::Agent,
                    text: "Moved the cursor guard ahead of the update.".into(),
                });
            }
            let total_entries = entries.len();
            Ok(Response::SharePreview(SharePreview {
                entries,
                truncated: false,
                total_entries,
            }))
        }
        Request::ShareCreate {
            pane,
            history,
            include_tools,
            lifetime_secs,
            relay,
        } => {
            let share_id = Uuid::new_v4();
            Ok(Response::ShareCreated(ShareCreated {
                share: ShareSummary {
                    share: share_id,
                    pane: pane.clone(),
                    history: *history,
                    include_tools: *include_tools,
                    created_ms: 1_000,
                    expires_ms: 1_000 + lifetime_secs * 1_000,
                    grants: 1,
                    relay: relay
                        .clone()
                        .unwrap_or_else(|| "https://relay.example.test".into()),
                },
                grant: Uuid::new_v4(),
                link: ShareLink::new("https://relay.example.test/share/#encoded-fragment".into()),
            }))
        }
        Request::ShareRevoke { .. } => Ok(Response::Done),
        _ => Ok(Response::Done),
    }
}

fn window(
    cx: &mut TestAppContext,
    enabled: bool,
) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (view, cx) = cx.add_window_view(fixture_window);
    let config = RemoteConfig {
        enabled,
        ..RemoteConfig::default()
    };
    cx.update(|_, cx| {
        Supervisor::fixture(cx, config.clone(), mock_daemon);
        if enabled {
            Supervisor::set_health(cx, Health::Running(fixture_status()), Some(Vec::new()));
        }
    });
    view.update(cx, |view, _| view.config.remote = config);
    (view, cx)
}

#[gpui::test]
fn share_dialog_uses_d5_defaults_previews_creates_and_stops(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true);
    let opened = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_share_session("w1:p1".into(), "Codex".into(), window, cx)
        })
    });
    assert!(opened);
    cx.run_until_parked();

    // D5 defaults: FromNow, tools excluded, 1 hour, empty FromNow preview.
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::ShareSession));
        let dialog = view.menu.remote_share.as_ref().unwrap();
        assert_eq!(dialog.history, ShareHistory::FromNow);
        assert!(!dialog.include_tools);
        assert_eq!(dialog.lifetime_secs, 3600);
        assert_eq!(dialog.selected_relay(), Some("https://relay.example.test"));
        match &dialog.preview {
            PreviewState::Ready(preview) => assert_eq!(preview.total_entries, 0),
            other => panic!("expected Ready preview, got {other:?}"),
        }
    });

    // Switching to Entire conversation + Include tool text refreshes preview.
    view.update(cx, |view, cx| view.set_share_history(ShareHistory::All, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let dialog = view.menu.remote_share.as_ref().unwrap();
        match &dialog.preview {
            PreviewState::Ready(preview) => assert_eq!(preview.total_entries, 2),
            other => panic!("expected 2-entry preview, got {other:?}"),
        }
    });

    view.update(cx, |view, cx| view.set_share_tools(true, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let dialog = view.menu.remote_share.as_ref().unwrap();
        match &dialog.preview {
            PreviewState::Ready(preview) => assert_eq!(preview.total_entries, 3),
            other => panic!("expected 3-entry preview, got {other:?}"),
        }
    });

    // Creating a link populates the one-time link and marks the pane shared.
    view.update(cx, |view, cx| view.create_share_link(cx));
    cx.run_until_parked();
    let share_id = view.read_with(cx, |view, cx| {
        let dialog = view.menu.remote_share.as_ref().unwrap();
        assert!(dialog.created.is_some());
        assert_eq!(dialog.active.len(), 1);
        assert!(Supervisor::is_pane_shared("w1:p1", cx));
        dialog.active[0].share
    });

    // Stopping the share clears the active entry and header indicator.
    view.update(cx, |view, cx| view.stop_share(share_id, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let dialog = view.menu.remote_share.as_ref().unwrap();
        assert!(dialog.created.is_none());
        assert!(dialog.active.is_empty());
        assert!(!Supervisor::is_pane_shared("w1:p1", cx));
    });
}

#[gpui::test]
fn share_dialog_explains_when_remote_control_is_off(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, false);
    let opened = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_share_session("w1:p1".into(), "Claude".into(), window, cx)
        })
    });
    assert!(opened);
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::ShareSession));
        let dialog = view.menu.remote_share.as_ref().unwrap();
        assert!(!dialog.enabled);
    });
}

#[test]
fn remote_share_cli_parses_shares_and_share_pane() {
    use crate::cli::{
        LaunchMode, LaunchOptions,
        control::{ControlCommand, RemoteCommand},
    };
    let shares = LaunchOptions::parse(["remote", "shares"]).unwrap();
    assert!(matches!(
        shares.mode,
        LaunchMode::Control(c) if matches!(c.command, ControlCommand::Remote(RemoteCommand::Shares))
    ));
    let share = LaunchOptions::parse(["remote", "share", "--pane", "w1:p2"]).unwrap();
    assert!(matches!(
        share.mode,
        LaunchMode::Control(c)
            if matches!(
                c.command,
                ControlCommand::Remote(RemoteCommand::Share { pane: Some(ref p) }) if p == "w1:p2"
            )
    ));
}
