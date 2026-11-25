//! Output that arrives after a Windows child's stdin has ended. OpenSSH for
//! Windows' `ssh.exe` delivers only its first write into a synchronous pipe
//! once its stdin has ended (#1563), so both runners give it an overlapped
//! pipe for stdout there.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use crate::{ScriptLimits, run_script_command};
use std::{
    io,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const LIMITS: ScriptLimits = ScriptLimits {
    output: 1 << 20,
    idle: Duration::from_secs(20),
};

/// A local child that writes, pauses, and writes again after its stdin has
/// ended is read in full through the runner's stdout, and its exit is seen.
#[cfg(windows)]
#[test]
fn every_write_after_stdin_ends_is_read() {
    let mut command = std::process::Command::new("cmd.exe");
    command.args(["/c", "echo a& ping -n 2 127.0.0.1 >nul& echo b"]);
    let mut output = Vec::new();
    let read = run_script_command(
        command,
        io::empty(),
        &mut output,
        LIMITS,
        &AtomicBool::new(false),
    )
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(text.split_whitespace().collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(read, text.len() as u64);
}

/// The script the live tests run on a POSIX host: three lines, apart.
const TICKS: &str = "for i in 0 1 2; do sleep 1; echo t$i; done";

fn ticks(text: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(text)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// A real SSH host running a POSIX shell, named by `SHEPRD_TEST_SSH`: every
/// line a script prints over several writes arrives, after the runner has
/// closed its stdin.
#[test]
#[ignore = "needs an SSH host named in SHEPRD_TEST_SSH"]
fn a_script_over_ssh_is_read_in_full() {
    let target = std::env::var("SHEPRD_TEST_SSH").expect("SHEPRD_TEST_SSH");
    let mut output = Vec::new();
    let started = Instant::now();
    run_script_command(
        crate::script_command(&target, TICKS).unwrap(),
        io::empty(),
        &mut output,
        LIMITS,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        ticks(&output),
        ["t0", "t1", "t2"],
        "{:?}",
        started.elapsed()
    );
}

/// The same through the probe runner, whose child has no stdin at all.
#[test]
#[ignore = "needs an SSH host named in SHEPRD_TEST_SSH"]
fn a_probe_over_ssh_is_read_in_full() {
    let target = std::env::var("SHEPRD_TEST_SSH").expect("SHEPRD_TEST_SSH");
    let mut command = crate::script_command(&target, TICKS).unwrap();
    let (status, output) =
        crate::ssh::run(&mut command, Duration::from_secs(30), || false).unwrap();
    assert!(status.success());
    assert_eq!(ticks(&output), ["t0", "t1", "t2"]);
}
