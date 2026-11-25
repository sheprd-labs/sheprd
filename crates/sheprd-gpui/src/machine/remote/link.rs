//! The borrowing side: one request to the Sheprd on another computer, by
//! running `sheprd machine lend` there over the host connection (SSH, which
//! shares one login per host). It blocks, so it runs on a worker.
//!
//! The answer is untrusted: bounded, parsed strictly, and its pictures
//! checked before they are written or drawn.

use super::Error;
use crate::{
    control::protocol::lend::{LendAnswer, LendCall, MAX_LEND_ANSWER},
    teleport::Host,
};
use sheprd_client::ScriptLimits;
use std::{sync::LazyLock, sync::atomic::AtomicBool, time::Duration};

// Exit codes of `docs/cli.md`, and a shell's for a missing program.
const USAGE: i32 = 2;
const NOT_RUNNING: i32 = 3;
const NOT_FOUND: i32 = 127;

/// What `sheprd machine lend` is run as: no `--json`, so its answer is the
/// bytes it writes.
const LEND: &[&str] = &["machine", "lend"];

/// This computer's name, which the lending computer shows as the borrower.
pub(crate) static BORROWER: LazyLock<String> = LazyLock::new(|| {
    sysinfo::System::host_name()
        .map(|name| crate::notifications::safe_text(&name, 64))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "another computer".to_owned()) // i18n: data
});

/// Sends `call` to the Sheprd on `host`, called `label` in errors, waiting
/// at most `wait` for its answer.
pub(crate) fn run(
    host: &Host,
    label: &str,
    call: &LendCall,
    wait: Duration,
    cancelled: &AtomicBool,
) -> Result<LendAnswer, Error> {
    let request = serde_json::to_vec(call).map_err(|source| Error::Request {
        host: label.to_owned(),
        source,
    })?;
    let limits = ScriptLimits {
        output: MAX_LEND_ANSWER as u64,
        idle: wait,
    };
    let mut output = Vec::new();
    let script = crate::remote_update::script(LEND);
    match host.stream(&script, request.as_slice(), &mut output, limits, cancelled) {
        Ok(_) => parse(label, &output),
        Err(error) => Err(classify(label, error)),
    }
}

/// One answer from `output`.
pub(crate) fn parse(label: &str, output: &[u8]) -> Result<LendAnswer, Error> {
    if output.len() > MAX_LEND_ANSWER {
        return Err(Error::TooLarge(label.to_owned()));
    }
    serde_json::from_slice(output).map_err(|source| Error::Answer {
        host: label.to_owned(),
        source,
    })
}

/// A failed run, by the exit code of the command line there.
pub(crate) fn classify(label: &str, error: sheprd_client::Error) -> Error {
    let code = match &error {
        sheprd_client::Error::ScriptExit { status, .. } => status.code(),
        _ => None,
    };
    let host = label.to_owned();
    match code {
        Some(NOT_FOUND) => Error::Missing(host),
        Some(NOT_RUNNING) => Error::NotRunning(host),
        // An app from before lending takes no such verb.
        Some(USAGE) => Error::TooOld(host),
        _ => Error::Run {
            host,
            source: error,
        },
    }
}

#[cfg(test)]
mod tests;
