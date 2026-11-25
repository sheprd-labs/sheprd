//! Whether each runner hands its Windows child overlapped stdout and stderr
//! handles, as `ssh.exe` needs (#1563), asked of the child itself: this test
//! binary, run again as the child, reports its own standard handles. Unlike
//! the live SSH tests, this runs everywhere and fails if a runner goes back
//! to synchronous pipes.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use crate::{ScriptLimits, run_script_command};
use std::{
    io::{self, Write},
    os::windows::io::{AsRawHandle, RawHandle},
    process::Command,
    sync::atomic::AtomicBool,
    time::Duration,
};

/// Set only in the child, so the report below does nothing in a plain run.
const REPORT: &str = "SHEPRD_TEST_REPORT_STANDARD_HANDLES";
const EXPECTED: &str = "handles stdout=overlapped stderr=overlapped";

/// Whether `handle` was opened for overlapped I/O: only such a handle can be
/// associated with an I/O completion port, and any other is refused with
/// `ERROR_INVALID_PARAMETER`.
fn overlapped(handle: RawHandle) -> bool {
    use windows_sys::Win32::{Foundation::CloseHandle, System::IO::CreateIoCompletionPort};
    // SAFETY: `handle` is one of this process's standard handles, open for
    // the process's life. A new port is created for it and closed again, and
    // no overlapped operation is ever started on it, so nothing is posted to
    // the port. The standard library offers no way to ask how a handle was
    // opened.
    #[allow(unsafe_code)]
    unsafe {
        let port = CreateIoCompletionPort(handle, std::ptr::null_mut(), 0, 1);
        if port.is_null() {
            return false;
        }
        CloseHandle(port);
        true
    }
}

/// The child's side, run by [`reporter`]: prints how its stdout and stderr
/// were opened.
#[test]
#[ignore = "run as a child by the tests below"]
fn report_standard_handles() {
    if std::env::var_os(REPORT).is_none() {
        return;
    }
    let mode = |handle| {
        if overlapped(handle) {
            "overlapped"
        } else {
            "synchronous"
        }
    };
    let (stdout, stderr) = (
        mode(io::stdout().as_raw_handle()),
        mode(io::stderr().as_raw_handle()),
    );
    let mut out = io::stdout().lock();
    writeln!(out, "handles stdout={stdout} stderr={stderr}").unwrap();
    out.flush().unwrap();
}

/// This test binary, running only [`report_standard_handles`].
fn reporter() -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "script::overlapped_handles_tests::report_standard_handles",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(REPORT, "1");
    command
}

#[test]
fn the_script_runner_gives_its_child_overlapped_output() {
    let mut output = Vec::new();
    let limits = ScriptLimits {
        output: 1 << 20,
        idle: Duration::from_secs(60),
    };
    run_script_command(
        reporter(),
        io::empty(),
        &mut output,
        limits,
        &AtomicBool::new(false),
    )
    .unwrap();
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains(EXPECTED), "{text}");
}

#[test]
fn the_probe_runner_gives_its_child_overlapped_output() {
    let (status, output) =
        crate::ssh::run(&mut reporter(), Duration::from_secs(60), || false).unwrap();
    let text = String::from_utf8_lossy(&output);
    assert!(status.success(), "{status:?}: {text}");
    assert!(text.contains(EXPECTED), "{text}");
}
