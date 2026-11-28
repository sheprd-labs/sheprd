//! The last words of a chat link's child: `ssh` and the `sheprd-acp
//! connect` it runs say why a connection failed only on standard error,
//! such as a relay refusing the dial or the program missing on the host.
//! The pipe is drained to its end, so the child never blocks on it, and
//! only its last bytes are kept, made printable: they come from another
//! machine and go into the log.

use std::{
    io::Read,
    process::ChildStderr,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

/// The most bytes kept from the end of the child's standard error.
const MAX_TAIL: usize = 512;
/// How long the link waits, once its child is reaped, for the rest of the
/// pipe: a process the child left behind may still hold it open.
const WAIT: Duration = Duration::from_millis(250);

/// The end of a child's standard error, once it closes.
pub(super) struct Tail(Receiver<String>);

impl Tail {
    /// Starts draining `stderr` on a thread of its own; `None` when no
    /// thread could be started, and the pipe is then dropped.
    pub(super) fn watch(mut stderr: ChildStderr) -> Option<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("herdr-gui-chat-stderr".into())
            .spawn(move || {
                let mut tail = Vec::with_capacity(MAX_TAIL);
                let mut buffer = [0; 4096];
                loop {
                    match stderr.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            tail.extend_from_slice(&buffer[..read]);
                            let excess = tail.len().saturating_sub(MAX_TAIL);
                            tail.drain(..excess);
                        }
                    }
                }
                let _ = sender.send(printable(&tail));
            })
            .ok()?;
        Some(Self(receiver))
    }

    /// What the child said, if anything, waiting at most [`WAIT`].
    pub(super) fn said(self) -> Option<String> {
        self.0
            .recv_timeout(WAIT)
            .ok()
            .filter(|text| !text.is_empty())
    }
}

/// `bytes` as one line: invalid UTF-8 and control characters replaced, line
/// breaks joined with ` | `.
fn printable(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| {
            line.chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

// The tests run `/bin/sh`.
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn a_childs_last_words_are_kept_printable() {
        let mut child = Command::new("/bin/sh")
            .args([
                "-c",
                // i18n: data
                "printf 'noise\\n%.0s' $(seq 200) >&2; printf 'relay-connect: the relay answered Limited\\033[31m\\n' >&2; exit 1",
            ])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let tail = Tail::watch(child.stderr.take().unwrap()).unwrap();
        child.wait().unwrap();
        let said = tail.said().unwrap();
        assert!(
            said.ends_with("relay-connect: the relay answered Limited [31m"),
            "{said}"
        );
        assert!(said.len() <= MAX_TAIL + MAX_TAIL / 2, "{}", said.len());
        assert!(!said.contains('\u{1b}'));
    }

    #[test]
    fn a_quiet_child_says_nothing() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let tail = Tail::watch(child.stderr.take().unwrap()).unwrap();
        child.wait().unwrap();
        assert_eq!(tail.said(), None);
    }
}
