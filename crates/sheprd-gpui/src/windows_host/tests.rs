#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn answers_are_taken_one_at_a_time() {
    let mut buffer = b"cores 2\r\n@@herdr-answer-end\r\ncores 3\n@@herdr-answer".to_vec();
    assert_eq!(take_answer(&mut buffer).as_deref(), Some("cores 2\r\n"));
    assert_eq!(take_answer(&mut buffer), None, "the end line is incomplete");
    buffer.extend_from_slice(b"-end\n");
    assert_eq!(take_answer(&mut buffer).as_deref(), Some("cores 3\n"));
    assert!(buffer.is_empty());
    // The marker counts only as a whole line.
    let mut buffer = b"x @@herdr-answer-end\n".to_vec();
    assert_eq!(take_answer(&mut buffer), None);
}

#[test]
fn only_the_newest_answer_is_taken() {
    let mut buffer =
        b"cores 1\n@@herdr-answer-end\ncores 2\r\n@@herdr-answer-end\r\ncores 3\n".to_vec();
    assert_eq!(take_latest(&mut buffer).as_deref(), Some("cores 2\r\n"));
    assert_eq!(buffer, b"cores 3\n", "an answer still arriving is kept");
    assert_eq!(take_latest(&mut buffer), None);
}

#[test]
fn a_script_wraps_its_lines_in_the_loop() {
    let text = script(
        "$herdrSetup = 1",
        "    $herdrBody = 2",
        Duration::from_secs(5),
    );
    let setup = text.find("$herdrSetup = 1").unwrap();
    let body = text.find("$herdrBody = 2").unwrap();
    let end = text.find("AppendLine('@@herdr-answer-end')").unwrap();
    assert!(
        text.find("'sshd*'").unwrap() < setup,
        "the watch comes first"
    );
    assert!(setup < body && body < end);
    assert!(text.contains("Start-Sleep -Seconds 5\n"));
    assert!(!text.contains("__"), "every placeholder is filled");
    // A period under a second still sleeps, rather than spinning.
    assert!(script("", "", Duration::ZERO).contains("Start-Sleep -Seconds 1\n"));
}

/// A process that exits before answering is closed, not waited on until the
/// timeout.
#[test]
fn an_exited_process_is_closed_at_once() {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("cmd.exe");
        command.args(["/c", "exit 3"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "exit 3"]);
        command
    };
    let mut session = Session::start(&mut command).unwrap();
    let started = Instant::now();
    assert!(matches!(
        session.sample(Duration::from_secs(30)),
        Err(crate::Error::WindowsHost(Error::Closed))
    ));
    assert!(started.elapsed() < Duration::from_secs(10));
}

/// Answers are read as they come, also through a Windows client's spool.
#[test]
fn answers_arrive_while_the_process_runs() {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg(script(
                "",
                "    [void]$herdrOut.AppendLine('tick')",
                Duration::from_secs(1),
            ));
        command.stderr(Stdio::null());
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = std::process::Command::new("/bin/sh");
        command.args([
            "-c",
            "while :; do printf 'tick\\n@@herdr-answer-end\\n'; sleep 1; done",
        ]);
        command
    };
    let mut session = Session::start(&mut command).unwrap();
    let limit = Duration::from_secs(60);
    for _ in 0..2 {
        assert_eq!(session.sample(limit).unwrap().trim(), "tick");
    }
}

/// The command line sshd hands the login shell for `script`, which
/// `cmd.exe` refuses beyond [`COMMAND_LIMIT`].
pub(crate) fn remote_command_length(script: &str) -> usize {
    let bootstrap = compressed(script).unwrap();
    // `powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand `,
    // then the bootstrap as UTF-16 in base64.
    66 + (bootstrap.encode_utf16().count() * 2).div_ceil(3) * 4
}

#[test]
fn a_small_script_fits_the_command_line() {
    assert!(remote_command_length(&script("", "", Duration::from_secs(2))) < COMMAND_LIMIT);
}

/// The compressed form, run by this machine's own PowerShell, runs the
/// script it carries.
#[cfg(windows)]
#[test]
fn a_compressed_script_runs_as_itself() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let text = script(
        "$herdrWord = 'it''s here'",
        "    [void]$herdrOut.AppendLine($herdrWord)",
        Duration::from_secs(1),
    );
    let bootstrap = compressed(&text).unwrap();
    let utf16: Vec<u8> = bootstrap
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut command = std::process::Command::new("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
        ])
        .arg(STANDARD.encode(utf16))
        .stderr(Stdio::null());
    let mut session = Session::start(&mut command).unwrap();
    let limit = Duration::from_secs(60);
    for _ in 0..2 {
        assert_eq!(session.sample(limit).unwrap().trim(), "it's here");
    }
}

/// The first answer may take longer than later ones (PowerShell loading
/// modules, a script compiling C#): it is waited for past `timeout`, so a
/// slow machine is not restarted forever.
#[test]
fn a_slow_first_answer_is_waited_for() {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg(script(
                "Start-Sleep -Seconds 3",
                "    [void]$herdrOut.AppendLine('late')",
                Duration::from_secs(1),
            ));
        command.stderr(Stdio::null());
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = std::process::Command::new("/bin/sh");
        command.args([
            "-c",
            "sleep 3; while :; do printf 'late\n@@herdr-answer-end\n'; sleep 1; done",
        ]);
        command
    };
    let mut session = Session::start(&mut command).unwrap();
    let step = Duration::from_millis(500);
    assert_eq!(session.sample(step).unwrap().trim(), "late");
    // Later answers keep to the step, a period apart.
    assert_eq!(
        session.sample(Duration::from_secs(30)).unwrap().trim(),
        "late"
    );
}

const MARK: &[u8] = b"@@herdr-answer-end\n";

#[test]
fn a_chunk_completing_several_answers_keeps_the_newest() {
    let mut buffer = Vec::new();
    let chunk = [b"a\n".as_slice(), MARK, b"b\n", MARK, b"c"].concat();
    assert_eq!(add(&mut buffer, &chunk).unwrap().as_deref(), Some("b\n"));
    assert_eq!(buffer, b"c");
}

/// A finished answer larger than [`LIMIT`] is refused, not returned.
#[test]
fn an_oversized_answer_is_refused() {
    let mut buffer = Vec::new();
    let chunk = [vec![b'x'; LIMIT + 1].as_slice(), b"\n", MARK].concat();
    assert!(matches!(
        add(&mut buffer, &chunk),
        Err(crate::Error::WindowsHost(Error::Size))
    ));
}

/// An answer that never ends is refused once it passes [`LIMIT`], so the
/// buffer never holds more than that and one chunk.
#[test]
fn an_unfinished_answer_is_refused_past_the_limit() {
    let mut buffer = Vec::new();
    let chunk = vec![b'x'; 8192];
    let mut added = 0;
    let error = loop {
        match add(&mut buffer, &chunk) {
            Ok(None) => added += chunk.len(),
            Ok(Some(_)) => panic!("no answer ended"),
            Err(error) => break error,
        }
        assert!(added <= LIMIT, "{added}");
    };
    assert!(matches!(error, crate::Error::WindowsHost(Error::Size)));
    assert!(buffer.len() <= LIMIT + chunk.len());
}

/// A process that prints without end is refused rather than read forever.
#[test]
fn a_process_that_never_stops_printing_is_refused() {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg("$x = 'x' * 8000; while ($true) { [Console]::Out.Write($x) }");
        command.stderr(Stdio::null());
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "yes xxxxxxxxxxxxxxxx"]);
        command
    };
    let mut session = Session::start(&mut command).unwrap();
    let started = Instant::now();
    assert!(matches!(
        session.sample(Duration::from_secs(60)),
        Err(crate::Error::WindowsHost(Error::Size))
    ));
    assert!(started.elapsed() < Duration::from_secs(60));
}

/// A real Windows SSH host, named by `SHEPRD_TEST_WINDOWS_SSH`: a script
/// whose session is dropped stops on the host, also when this process holds
/// other connections to it, as a GPU setting change replacing the load script
/// does (#1560).
#[test]
#[ignore = "needs a Windows SSH host named in SHEPRD_TEST_WINDOWS_SSH"]
fn a_dropped_session_stops_its_script_on_the_host() {
    let target = std::env::var("SHEPRD_TEST_WINDOWS_SSH").unwrap();
    let pid_of = |session: &mut Session| -> u32 {
        let answer = session.sample(Duration::from_secs(30)).unwrap();
        answer.trim().parse().unwrap()
    };
    let pid_script = script(
        "",
        "    [void]$herdrOut.AppendLine($PID)",
        Duration::from_secs(1),
    );
    // Kept open throughout, so the host keeps a connection from this process.
    let mut kept = Session::connect(&target, &pid_script).unwrap();
    let kept_pid = pid_of(&mut kept);
    let mut dropped = Session::connect(&target, &pid_script).unwrap();
    let dropped_pid = pid_of(&mut dropped);
    assert_ne!(kept_pid, dropped_pid);
    drop(dropped);
    // Asked through a session of its own: a one-shot `ssh` on Windows stalls
    // writing its output into a synchronous pipe.
    let check = format!(
        "    [void]$herdrOut.AppendLine($(if (Get-Process -Id {dropped_pid}) {{ 'running' }} else {{ 'gone' }}))"
    );
    let mut checker =
        Session::connect(&target, &script("", &check, Duration::from_secs(1))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let state = checker.sample(Duration::from_secs(30)).unwrap();
        if state.trim() == "gone" {
            break;
        }
        assert_eq!(state.trim(), "running");
        assert!(Instant::now() < deadline, "the dropped script kept running");
    }
    assert_eq!(
        pid_of(&mut kept),
        kept_pid,
        "the other script still answers"
    );
}
