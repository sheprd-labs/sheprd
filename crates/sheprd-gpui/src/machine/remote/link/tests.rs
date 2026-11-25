#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::{ErrorCode, Response};

#[test]
fn reads_an_answer_and_refuses_anything_else() {
    let answer = LendAnswer::Response {
        response: Response::error(ErrorCode::Busy, "busy"),
    };
    let text = serde_json::to_vec(&answer).unwrap();
    assert_eq!(parse("mini", &text).unwrap(), answer);
    assert!(matches!(parse("mini", b"{}"), Err(Error::Answer { .. })));
    assert!(matches!(
        parse("mini", b"Usage: sheprd"),
        Err(Error::Answer { .. })
    ));
}

#[cfg(unix)]
#[test]
fn exit_codes_say_what_is_missing_there() {
    use std::os::unix::process::ExitStatusExt as _;
    let exit = |code: i32| sheprd_client::Error::ScriptExit {
        status: std::process::ExitStatus::from_raw(code << 8),
        stderr: String::new(),
    };
    assert!(matches!(classify("mini", exit(127)), Error::Missing(_)));
    assert!(matches!(classify("mini", exit(3)), Error::NotRunning(_)));
    assert!(matches!(classify("mini", exit(2)), Error::TooOld(_)));
    assert!(matches!(classify("mini", exit(255)), Error::Run { .. }));
}
