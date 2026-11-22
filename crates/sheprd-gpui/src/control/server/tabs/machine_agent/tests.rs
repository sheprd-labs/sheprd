#![allow(clippy::panic)]
use super::*;

fn exec(status: Option<i32>, timed_out: bool) -> agent::Exec {
    agent::Exec {
        status,
        stdout: "partial\n".into(),
        stderr: String::new(),
        truncated: false,
        timed_out,
    }
}

/// A command the agent stopped at its timeout answers as `cloud exec`
/// does, exit status 7, rather than as a command a signal ended.
#[test]
fn a_command_stopped_at_its_timeout_reports_a_timeout() {
    match executed("vm".into(), exec(None, true), 2) {
        Response::Error { code, message } => {
            assert_eq!(code, ErrorCode::Timeout);
            assert!(message.contains("2 seconds"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_finished_command_hands_back_its_status_and_output() {
    let Response::Tab {
        reply: Reply::MachineExec(info),
    } = executed("vm".into(), exec(Some(3), false), 2)
    else {
        panic!("not an exec reply");
    };
    assert_eq!(info.status, Some(3));
    assert_eq!(info.stdout, "partial\n");
    assert_eq!(info.device, "vm");
}
