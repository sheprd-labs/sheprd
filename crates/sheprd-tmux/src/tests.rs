#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use std::{io::Cursor, sync::mpsc};

#[test]
fn a_native_child_pane_takes_precedence_over_its_tmux_parent() {
    let tmux = Tmux::named_id("parent").unwrap();
    for (nested, expected) in [
        ("", Some(tmux.clone())),
        ("HERDR_PANE_ID", None),
        ("HC_TERMINAL_ID", None),
    ] {
        let caller = Tmux::caller_in(|key| match key {
            "SHEPRD_TERMINAL_ID" => Some(tmux.to_string()),
            key if key == nested => Some("child".into()),
            _ => None,
        });
        assert_eq!(caller, expected);
    }
}

#[test]
fn a_saved_id_cannot_target_other_sessions() {
    let id = Tmux::named_id("shell").unwrap();
    assert!(id.is_tmux());
    for bad in ["shell", "t1", "%0", "tmux-*", "tmux-1:0", "tmux-$(x)"] {
        assert!(Tmux::target(&TerminalId::new(bad).unwrap()).is_err());
    }
}

#[test]
fn capture_text_is_never_treated_as_a_notification() {
    let (tx, rx) = mpsc::sync_channel(16);
    let sentinel = "sentinel-test-nonce";
    let stream = [
        "%begin 1 1 0",
        "%end 1 1 0",
        "%begin 1 2 1",
        "0 3 80 0",
        "%end 1 2 1",
        "%begin 1 3 1",
        "%end 1 3 1",
        "%begin 2 2 1",
        "%output %1 hello",
        "%end 2 2 1",
        "%begin 9 9 1",
        "%end 2 2 1",
        "%begin 2 3 1",
        sentinel,
        "%end 2 3 1",
        "%begin 3 3 1",
        "3 2 1",
        "%end 3 3 1",
        "%output %2 wrong",
        r"%output %1 \033\377x",
        "%exit",
        "",
    ]
    .join("\n");
    control::read(
        Cursor::new(stream.as_bytes()),
        "%1",
        100,
        sentinel,
        &tx,
        None,
    )
    .unwrap();
    drop(tx);
    let bytes: Vec<_> = rx
        .into_iter()
        .filter_map(|m| {
            if let control::Message::Bytes(b) = m {
                Some(b)
            } else {
                None
            }
        })
        .flatten()
        .collect();
    assert_eq!(
        bytes,
        b"%output %1 hello\r\n%end 2 2 1\r\n%begin 9 9 1\x1b[0m\x1b[3;4H\x1b[?1h\x1b\xffx"
    );
}

#[test]
fn capture_error_aborts_through_the_trailing_sentinel() {
    let (tx, _rx) = mpsc::sync_channel(16);
    let sentinel = "sentinel-err-nonce";
    let stream = [
        "%begin 1 2 1",
        "0 2 80 0",
        "%end 1 2 1",
        "%begin 1 3 1",
        "%end 1 3 1",
        "%begin 2 2 1",
        "can't find pane: %1",
        "%error 2 2 1",
        "%begin 2 3 1",
        sentinel,
        "%end 2 3 1",
        "",
    ]
    .join("\n");
    assert!(
        control::read(
            Cursor::new(stream.as_bytes()),
            "%1",
            100,
            sentinel,
            &tx,
            None,
        )
        .is_err()
    );
}

#[test]
fn joined_capture_preserves_wraps_distinct_from_newlines() {
    use sheprd_backend::{History, HistoryAnswer};
    let (tx, rx) = mpsc::sync_channel(16);
    let sentinel = "sentinel-wrap-nonce";
    // Width 10, height 4, history 1: first line wraps across history into row 0,
    // second line wraps on visible rows 1..2, third line is a single row.
    let stream = [
        "%begin 1 2 1",
        "1 4 10 0",
        "%end 1 2 1",
        "%begin 1 3 1",
        "%end 1 3 1",
        "%begin 2 2 1",
        "1234567890abcdef",
        "ABCDEFGHIJklmn",
        "short",
        "%end 2 2 1",
        "%begin 2 3 1",
        sentinel,
        "%end 2 3 1",
        "%begin 3 3 1",
        "5 3",
        "%end 3 3 1",
        "%exit",
        "",
    ]
    .join("\n");
    control::read(
        Cursor::new(stream.as_bytes()),
        "%1",
        100,
        sentinel,
        &tx,
        None,
    )
    .unwrap();
    drop(tx);
    let mut emulator = sheprd_terminal::Emulator::new(Size::new(10, 4).unwrap(), 100);
    for msg in rx {
        match msg {
            control::Message::Resize(size) => emulator.resize(size),
            control::Message::Bytes(bytes) => emulator.feed(&bytes),
            control::Message::Exit(_) | control::Message::Failed(_) => {}
        }
    }
    let HistoryAnswer::Text(text) = emulator.history(&History::Text).unwrap() else {
        panic!("expected text");
    };
    assert!(
        text.contains("1234567890abcdef\nABCDEFGHIJklmn\nshort"),
        "expected wrapped rows without inserted newlines: {text:?}"
    );
    assert_eq!(emulator.line_rows(), vec![2, 2, 1]);
    assert_eq!(emulator.wrapped_rows(), vec![false, true, false, false]);
}

#[test]
fn full_input_queue_waits_and_drains_output_without_disconnecting() {
    use sheprd_backend::{ClientPaneInputEvent, History, HistoryAnswer, Terminal};
    let (cmd_tx, cmd_rx) = mpsc::sync_channel(1);
    let (out_tx, out_rx) = mpsc::sync_channel(1);
    let mut terminal = TmuxTerminal::for_test(cmd_tx, out_rx, Size::new(40, 4).unwrap());
    let reader = std::thread::spawn(move || {
        // Send output first so the sender must drain output while waiting on
        // the full 1-slot command channel (avoiding a stdin/stdout deadlock).
        out_tx
            .send(control::Message::Bytes(b"echoed".to_vec()))
            .unwrap();
        let mut received = Vec::new();
        for _ in 0..8 {
            if let terminal::Command::Keys(bytes) = cmd_rx.recv().unwrap() {
                received.extend(bytes);
            }
        }
        out_tx
            .send(control::Message::Exit(sheprd_backend::Exit {
                code: Some(0),
            }))
            .unwrap();
        received
    });
    for byte in b"abcdefgh" {
        terminal
            .input(&ClientPaneInputEvent::TextCommit(String::from(
                *byte as char,
            )))
            .unwrap();
    }
    assert_eq!(reader.join().unwrap(), b"abcdefgh");
    let updates = terminal.next(Duration::from_millis(200)).unwrap();
    assert!(
        updates
            .iter()
            .any(|u| matches!(u, sheprd_backend::SurfaceUpdate::Ended(e) if e.code == Some(0)))
    );
    let HistoryAnswer::Text(text) = terminal.history(&History::Text).unwrap() else {
        panic!("expected text");
    };
    assert!(text.contains("echoed"), "screen missing output: {text:?}");
}

#[cfg(unix)]
#[test]
fn remote_shell_round_trips_every_argument() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("tmux");
    std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\0' \"$@\"\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let args = [
        "new-session",
        "a b",
        "it's here",
        "$HOME",
        ";",
        "two\nlines",
        "",
    ];
    let mut command = sheprd_client::new_command("tmux");
    command.args(args);
    let output = sheprd_client::new_command("/bin/sh")
        .env_clear()
        .env("PATH", dir.path())
        .arg("-c")
        .arg(host::remote_script(&command).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut expected = args.join("\0").into_bytes();
    expected.push(0);
    assert_eq!(output.stdout, expected);
}

#[test]
fn chat_discovery_has_the_reporting_process_identity() {
    let id = Tmux::named_id("chat").unwrap();
    let report = AgentReport {
        pid: 42,
        display: "Codex".into(),
        status: AgentStatus::Idle,
        title: None,
        endpoint: Some("0123456789abcdef0123456789abcdef".into()),
    };
    let metadata = URL_SAFE_NO_PAD.encode(b"[null,{}]");
    let report = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&report).unwrap());
    let row = format!("{id}|0||{metadata}|{report}|sheprd-acp|/tmp/a|b");
    let info = record(&row).unwrap();
    assert_eq!(info.cwd, Some("/tmp/a|b".into()));
    assert_eq!(info.foreground.unwrap().pid, 42);
    assert_eq!(
        info.labels[sheprd_chat_protocol::TOKEN],
        "0123456789abcdef0123456789abcdef"
    );
    assert!(
        record(&row.replace("|sheprd-acp|", "|sh|"))
            .unwrap()
            .foreground
            .is_none()
    );
}

#[cfg(all(unix, feature = "integration-test"))]
#[allow(dead_code, clippy::disallowed_methods)]
#[path = "../../test-support/sandbox.rs"]
mod sandbox;

#[cfg(all(unix, feature = "integration-test"))]
mod lifecycle;
