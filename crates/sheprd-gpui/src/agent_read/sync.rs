//! Meeting a device's own daemon, which keeps what was read of its agents
//! (`sheprd_remote_control::control::read`). This computer's is one socket
//! away; another machine's is asked over SSH, as favorites are. Everything
//! here blocks: run it on the background executor.

use super::Error;
use crate::{device_tags::Daemon, remote, teleport::Host};
use sheprd_client::ScriptLimits;
use sheprd_remote_control::{
    app::read,
    control::read::{Change, MAX_MARKS, Unread},
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// What `read` may print: one JSON answer of at most `MAX_MARKS` panes.
const OUTPUT: u64 = 512 * 1024;
const IDLE: Duration = Duration::from_secs(20);

/// Finds the program without installing it. An older build refuses `read`
/// and prints nothing, as does a machine without the program or without a
/// daemon running. Standard input is drained so the sender is never left
/// writing to no one.
const READ: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" read 2>/dev/null || true; fi
cat >/dev/null
";
/// [`READ`], making the change given on standard input.
const CHANGE: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" read --offer 2>/dev/null || true; fi
cat >/dev/null
";

/// Makes `change`, if any, on the device's daemon and returns what is
/// unread there, or `None` when the device has no daemon that keeps marks.
pub(super) fn exchange(
    daemon: &Daemon,
    change: Option<Change>,
    cancelled: &AtomicBool,
) -> Result<Option<Unread>, Error> {
    match daemon {
        Daemon::Local => {
            let Ok(state_dir) = remote::state_dir() else {
                return Ok(None);
            };
            Ok(read::exchange(&state_dir, change)?)
        }
        Daemon::Remote(target) => {
            let host = Host::new(target).map_err(|_| Error::Unsupported)?;
            let (script, input) = match &change {
                Some(change) => (CHANGE, serde_json::to_vec(change).map_err(Error::Answer)?),
                None => (READ, Vec::new()),
            };
            let mut output = Vec::new();
            host.stream(
                script,
                input.as_slice(),
                &mut output,
                ScriptLimits {
                    output: OUTPUT,
                    idle: IDLE,
                },
                cancelled,
            )
            .map_err(Error::Run)?;
            parse(&output)
        }
    }
}

/// The host's answer, bounded; `None` when it printed nothing.
pub(super) fn parse(output: &[u8]) -> Result<Option<Unread>, Error> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let mut answer: Unread = serde_json::from_slice(output).map_err(Error::Answer)?;
    while answer.panes.len() > MAX_MARKS {
        answer.panes.pop_last();
    }
    Ok(Some(answer))
}
