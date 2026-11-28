#![allow(clippy::unwrap_used)]

use super::*;
use sheprd_chat_protocol::EntryId;
use sheprd_control_protocol::protocol::modal::{CueKind, ModalScope};
use std::time::{Duration, Instant};

#[test]
fn parses_all_interaction_directives_and_cues() {
    let (dir, title) = parse::opening(":::urgent Tap your YubiKey").unwrap();
    assert_eq!(dir, BlockDirective::Urgent);
    assert_eq!(title, "Tap your YubiKey");

    let urgent = parse::parse_block(
        dir,
        &title,
        "preset: yubikey\nWaiting on git push origin main",
    );
    assert_eq!(urgent.scope, ModalScope::All);
    assert_eq!(urgent.cue, Some(CueKind::Yubikey));
    assert!(urgent.auto_clear);
    assert_eq!(urgent.body, "Waiting on git push origin main");
    assert_eq!(parse::cue_line("::cue yubikey"), Some(CueKind::Yubikey));
    assert_eq!(parse::cue_line(":::cue yubikey"), None);

    let modal = parse::parse_block(
        BlockDirective::Modal,
        "Confirm push",
        "choices: Push | Open diff | Cancel\nreply: true\nReady to push.",
    );
    assert_eq!(modal.scope, ModalScope::Window);
    assert!(modal.auto_clear);
    match modal.payload {
        Payload::Modal {
            choices,
            quick_reply,
            ..
        } => {
            assert_eq!(choices, vec!["Push", "Open diff", "Cancel"]);
            assert!(quick_reply);
        }
        other => panic!("unexpected payload: {other:?}"),
    }

    let secret = parse::parse_block(
        BlockDirective::Secret,
        "Provide key",
        "key: STRIPE_TEST_KEY\ndest: env-file:.env.test.local\nNeeded for webhook tests.",
    );
    assert_eq!(secret.cue, Some(CueKind::Secret));
    match secret.payload {
        Payload::Secret { key, dest, .. } => {
            assert_eq!(key, "STRIPE_TEST_KEY");
            assert_eq!(dest, "env-file:.env.test.local");
        }
        other => panic!("unexpected payload: {other:?}"),
    }

    let runbook = parse::parse_block(
        BlockDirective::Runbook,
        "Ship v0.18",
        "- [x] agent: Run just ci\n- [ ] you: Tap YubiKey\n",
    );
    assert!(
        !runbook.auto_clear,
        "runbooks should stay pinned across chat turns by default"
    );
    match runbook.payload {
        Payload::Runbook { steps } => {
            assert_eq!(steps.len(), 2);
            assert!(steps[0].done && !steps[0].human);
            assert!(!steps[1].done && steps[1].human);
        }
        other => panic!("unexpected payload: {other:?}"),
    }

    let progress = parse::parse_block(
        BlockDirective::Progress,
        "Running tests",
        "percent: 68%\nstep: chat::effects",
    );
    assert_eq!(progress.scope, ModalScope::All);
    assert!(
        !progress.auto_clear,
        "progress pills should stay pinned across chat turns by default"
    );
    match progress.payload {
        Payload::Progress {
            percent,
            step,
            done,
        } => {
            assert_eq!(percent, Some(68));
            assert_eq!(step, "chat::effects");
            assert!(!done);
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn escalation_ladder_routes_by_presence_and_idle_time() {
    assert_eq!(
        escalate(ModalScope::All, Presence::InChat, 0, true),
        EscalationLevel::InlineOnly
    );
    assert_eq!(
        escalate(ModalScope::Window, Presence::SameWindowOtherTab, 0, true),
        EscalationLevel::WindowModal
    );
    assert_eq!(
        escalate(ModalScope::All, Presence::OtherApp, 0, true),
        EscalationLevel::GlobalHud
    );
    assert_eq!(
        escalate(ModalScope::Window, Presence::OtherApp, 5, true),
        EscalationLevel::WindowModal
    );
    assert_eq!(
        escalate(ModalScope::Window, Presence::OtherApp, 12, true),
        EscalationLevel::GlobalHud
    );
    assert_eq!(
        escalate(ModalScope::All, Presence::Away { idle_secs: 25 }, 25, true),
        EscalationLevel::PhoneEscalation
    );
    assert_eq!(
        escalate(ModalScope::All, Presence::OtherApp, 0, false),
        EscalationLevel::WindowModal
    );
}

#[test]
fn initial_snapshot_history_never_replays_saved_alerts_even_starting_at_entry_1() {
    let mut store = InteractionStore::default();
    let now = Instant::now();
    let win = WindowId::from(1u64);
    let urgent = parse::parse_block(BlockDirective::Urgent, "Tap YubiKey", "Old saved alert");

    // First snapshot load for a pane (entry IDs start at 1 in Projector).
    assert!(store.begin_pane_sync("p1", 1));
    assert!(!store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(1),
        revision: 1,
        history: true,
        presence: Presence::OtherApp,
        interactions: std::slice::from_ref(&urgent),
        cues: &[CueKind::Yubikey],
        now,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    assert!(store.summaries(now).is_empty());
    assert_eq!(store.active_edge_cue(now), None);

    // Subsequent poll on the same snapshot is live, and entry 1 is still seen.
    assert!(!store.begin_pane_sync("p1", 1));
    assert!(!store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(1),
        revision: 1,
        history: false,
        presence: Presence::OtherApp,
        interactions: std::slice::from_ref(&urgent),
        cues: &[CueKind::Yubikey],
        now,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    assert!(store.summaries(now).is_empty());
}

#[test]
fn streaming_chunks_update_in_place_and_never_reopen_dismissed_alerts() {
    let mut store = InteractionStore::default();
    let now = Instant::now();
    let win = WindowId::from(1u64);
    assert!(store.begin_pane_sync("p1", 1));

    let partial = parse::parse_block(BlockDirective::Urgent, "Tap YubiKey", "Waiting on");
    let complete = parse::parse_block(
        BlockDirective::Urgent,
        "Tap YubiKey",
        "Waiting on git push origin main",
    );

    // First streamed chunk creates modal.
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 1,
        history: false,
        presence: Presence::OtherApp,
        interactions: &[partial],
        cues: &[],
        now,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    let id = store.summaries(now)[0].id.clone();
    store.set_feedback(&id, "custom feedback".to_owned());

    // Second streamed chunk updates parsed body in-place without changing ID or feedback.
    let later = now + Duration::from_secs(2);
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 2,
        history: false,
        presence: Presence::OtherApp,
        interactions: std::slice::from_ref(&complete),
        cues: &[],
        now: later,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    let items = store.global_hud_items(later);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, id);
    assert_eq!(items[0].parsed.body, "Waiting on git push origin main");
    assert_eq!(items[0].created_at, now);
    assert_eq!(items[0].feedback.as_deref(), Some("custom feedback"));

    // User dismisses it; another streamed chunk of the same entry must NOT reopen it.
    assert!(store.dismiss(&id));
    assert!(!store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 3,
        history: false,
        presence: Presence::OtherApp,
        interactions: &[complete],
        cues: &[],
        now: later + Duration::from_secs(1),
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    assert!(store.summaries(later + Duration::from_secs(1)).is_empty());
}

#[test]
fn checked_runbook_steps_survive_unchanged_polls_and_streamed_revisions() {
    let mut store = InteractionStore::default();
    let now = Instant::now();
    let win = WindowId::from(1u64);
    assert!(store.begin_pane_sync("p1", 1));

    let runbook_v1 = parse::parse_block(
        BlockDirective::Runbook,
        "Ship v0.18",
        "- [x] agent: Run just ci\n- [ ] you: Tap YubiKey\n",
    );
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 1,
        history: false,
        presence: Presence::SameWindowOtherTab,
        interactions: std::slice::from_ref(&runbook_v1),
        cues: &[],
        now,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    let id = store.summaries(now)[0].id.clone();

    // User checks off step 1 ("Tap YubiKey").
    let toggled = store.toggle_runbook_step(&id, 1).unwrap();
    assert!(toggled.done);

    // Subsequent poll with the same revision must NOT overwrite the local check.
    assert!(!store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 1,
        history: false,
        presence: Presence::SameWindowOtherTab,
        interactions: &[runbook_v1],
        cues: &[],
        now: now + Duration::from_secs(1),
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));

    // Even a newer streamed revision adding another step preserves the locally checked step.
    let runbook_v2 = parse::parse_block(
        BlockDirective::Runbook,
        "Ship v0.18",
        "- [x] agent: Run just ci\n- [ ] you: Tap YubiKey\n- [ ] agent: Verify release\n",
    );
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 2,
        history: false,
        presence: Presence::SameWindowOtherTab,
        interactions: &[runbook_v2],
        cues: &[],
        now: now + Duration::from_secs(2),
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    let items = store.for_window(win, now + Duration::from_secs(2));
    match &items[0].parsed.payload {
        Payload::Runbook { steps } => {
            assert_eq!(steps.len(), 3);
            assert!(steps[1].done, "locally checked step must stay checked");
            assert!(!steps[2].done);
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn presence_routing_respects_in_chat_and_escalates_on_tab_or_app_switch() {
    let mut store = InteractionStore::default();
    let now = Instant::now();
    let win = WindowId::from(1u64);
    assert!(store.begin_pane_sync("p1", 1));

    let urgent = parse::parse_block(BlockDirective::Urgent, "Tap YubiKey", "Waiting on push");
    // Arrives while user is actively looking at p1: stays InlineOnly (no HUD or window modal).
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 1,
        history: false,
        presence: Presence::InChat,
        interactions: &[urgent],
        cues: &[],
        now,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    assert!(store.for_window(win, now).is_empty());
    assert!(store.global_hud_items(now).is_empty());
    let inline_id = match store.entry_modal_state("p1", EntryId(2), false) {
        EntryModalState::InlineOnly(id) => id,
        other => panic!("expected InlineOnly state, got {other:?}"),
    };

    // User switches to another tab in the same window -> promotes to WindowModal.
    assert!(store.refresh_presence(win, true, Some("p2"), true, now));
    assert_eq!(store.for_window(win, now).len(), 1);
    assert!(store.global_hud_items(now).is_empty());

    // User switches to another app -> promotes to GlobalHud.
    assert!(store.refresh_presence(win, false, Some("p2"), true, now));
    assert_eq!(store.global_hud_items(now).len(), 1);

    // User returns to p1 -> drops back to InlineOnly.
    assert!(store.refresh_presence(win, true, Some("p1"), true, now));
    assert!(store.for_window(win, now).is_empty());
    assert!(store.global_hud_items(now).is_empty());

    // Clicking "Open" on the inline chip promotes the same card directly without dismissing.
    assert!(store.open_card(&inline_id, false));
    assert_eq!(store.for_window(win, now).len(), 1);
    assert_eq!(store.for_window(win, now)[0].id, inline_id);
    assert_eq!(
        store.entry_modal_state("p1", EntryId(2), false),
        EntryModalState::Open(inline_id)
    );
}

#[test]
fn newer_entry_auto_clears_modals_but_preserves_progress_and_runbook() {
    let mut store = InteractionStore::default();
    let now = Instant::now();
    let win = WindowId::from(1u64);
    assert!(store.begin_pane_sync("p1", 1));

    let urgent = parse::parse_block(BlockDirective::Urgent, "Tap YubiKey", "Blocked on push");
    let progress = parse::parse_block(BlockDirective::Progress, "Building", "percent: 40%");
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(2),
        revision: 1,
        history: false,
        presence: Presence::OtherApp,
        interactions: &[urgent, progress],
        cues: &[],
        now,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    assert_eq!(store.summaries(now).len(), 2);

    // Newer entry clears urgent (`auto_clear = true`) while keeping progress (`auto_clear = false`).
    let later = now + Duration::from_secs(2);
    assert!(store.sync_chat_entry(ChatEntrySync {
        window_id: win,
        pane_id: "p1",
        agent_label: "Claude",
        workspace_label: "repo",
        workspace_root: None,
        entry_id: EntryId(3),
        revision: 1,
        history: false,
        presence: Presence::OtherApp,
        interactions: &[],
        cues: &[],
        now: later,
        volume: 0,
        window_modals_enabled: true,
        global_modals_enabled: true,
        progress_pill_enabled: true,
    }));
    let remaining = store.summaries(later);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].kind, "progress");
}

#[test]
fn vault_injects_0600_env_file_uses_private_credentials_store_and_redacts_debug() {
    let secret = vault::SecretValue::new("sk_test_super_secret_999".to_owned()).unwrap();
    assert!(!format!("{secret:?}").contains("sk_test_super_secret_999"));
    assert!(vault::parse_destination("env-file:../escape.env").is_err());
    assert!(vault::parse_destination("env-file:/etc/passwd").is_err());
    assert!(vault::parse_destination(r"env-file:\Windows\win.ini").is_err());
    assert!(vault::parse_destination("env-file:.").is_err());
    assert!(vault::validate_key("BAD KEY").is_err());

    let workspace = tempfile::tempdir().unwrap();
    let creds = tempfile::tempdir().unwrap();
    let dest = vault::parse_destination("env-file:.env.test.local").unwrap();

    // Missing workspace root must fail rather than falling back to temp dir.
    assert!(matches!(
        vault::inject_with_store(
            None,
            creds.path(),
            sheprd_credentials::Store::File,
            "STRIPE_TEST_KEY",
            &dest,
            &secret,
        ),
        Err(error::Error::WorkspaceNotFound)
    ));

    let receipt = vault::inject_with_store(
        Some(workspace.path()),
        creds.path(),
        sheprd_credentials::Store::File,
        "STRIPE_TEST_KEY",
        &dest,
        &secret,
    )
    .unwrap();
    assert!(!receipt.summary.contains("sk_test_super_secret_999"));

    let written = std::fs::read_to_string(workspace.path().join(".env.test.local")).unwrap();
    assert_eq!(written, "STRIPE_TEST_KEY=sk_test_super_secret_999\n");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(workspace.path().join(".env.test.local"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        // Keychain destination must store in credentials_dir (outside workspace),
        // never in a `.sheprd-vault` directory inside the project repo.
        let kc_dest = vault::parse_destination("keychain:stripe-dev").unwrap();
        let kc_receipt = vault::inject_with_store(
            Some(workspace.path()),
            creds.path(),
            sheprd_credentials::Store::File,
            "STRIPE_TEST_KEY",
            &kc_dest,
            &secret,
        )
        .unwrap();
        assert!(!kc_receipt.summary.contains("sk_test_super_secret_999"));
        assert!(!workspace.path().join(".sheprd-vault").exists());
        let saved_path = creds.path().join("agent-stripe-dev-STRIPE_TEST_KEY");
        assert!(saved_path.exists());
        let saved_mode = std::fs::metadata(&saved_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(saved_mode, 0o600);

        // Refuse symlink target.
        let link = workspace.path().join(".env.link");
        std::os::unix::fs::symlink(workspace.path().join(".env.test.local"), &link).unwrap();
        let link_dest = vault::parse_destination("env-file:.env.link").unwrap();
        assert!(matches!(
            vault::inject_with_store(
                Some(workspace.path()),
                creds.path(),
                sheprd_credentials::Store::File,
                "STRIPE_TEST_KEY",
                &link_dest,
                &secret,
            ),
            Err(error::Error::SymlinkRefused { .. })
        ));
    }
}

#[test]
fn audio_cue_synthesizer_produces_bounded_pcm_for_every_cue() {
    for &cue in CueKind::ALL {
        let samples = audio::synthesize_cue(cue);
        assert!(!samples.is_empty(), "{cue:?}");
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}

#[test]
fn cli_modal_commands_parse_cleanly() {
    use crate::cli::control::{ControlCommand, ModalCommand, parse};
    use sheprd_control_protocol::protocol::modal::ModalOp;
    use std::ffi::OsString;

    let run = |args: &[&str]| {
        parse(
            OsString::from(args[0]),
            args[1..].iter().map(OsString::from),
        )
        .unwrap()
        .command
    };

    assert_eq!(
        run(&["modal"]),
        ControlCommand::Modal(ModalCommand::Op(ModalOp::List))
    );
    assert_eq!(
        run(&["modal", "urgent", "--title", "Tap YubiKey", "git", "push"]),
        ControlCommand::Modal(ModalCommand::Op(ModalOp::Show {
            scope: ModalScope::All,
            title: "Tap YubiKey".to_owned(),
            body: "git push".to_owned(),
            preset: None,
            choices: Vec::new(),
            quick_reply: false,
            cue: None,
        }))
    );
    assert_eq!(
        run(&[
            "modal",
            "secret",
            "--key",
            "STRIPE_TEST_KEY",
            "--dest",
            "env-file:.env.test.local",
            "--save-keychain",
        ]),
        ControlCommand::Modal(ModalCommand::Op(ModalOp::Secret {
            title: "Provide STRIPE_TEST_KEY".to_owned(),
            key: "STRIPE_TEST_KEY".to_owned(),
            dest: "env-file:.env.test.local".to_owned(),
            reason: String::new(),
            save_keychain: true,
        }))
    );
    assert_eq!(
        run(&[
            "modal",
            "progress",
            "--title",
            "Tests",
            "--percent",
            "68",
            "--step",
            "effects"
        ]),
        ControlCommand::Modal(ModalCommand::Op(ModalOp::Progress {
            title: "Tests".to_owned(),
            percent: Some(68),
            step: "effects".to_owned(),
            done: false,
        }))
    );
    assert_eq!(
        run(&["modal", "cue", "yubikey"]),
        ControlCommand::Modal(ModalCommand::Op(ModalOp::Cue {
            cue: CueKind::Yubikey,
        }))
    );
    assert_eq!(
        run(&["modal", "clear", "--all"]),
        ControlCommand::Modal(ModalCommand::Op(ModalOp::Clear {
            all: true,
            id: None,
        }))
    );
}
