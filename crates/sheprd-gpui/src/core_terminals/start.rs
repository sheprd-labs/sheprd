//! A newly spawned terminal belongs to its waiting window only once its
//! result is handed back. A cancelled start must not leave an orphaned chat.

use super::Wanted;
use sheprd_backend::{TerminalId, TerminalInfo};
use std::sync::mpsc::{self, SyncSender};

pub(super) struct Outcome {
    result: Result<TerminalInfo, super::Error>,
    received: Option<SyncSender<()>>,
}

impl Outcome {
    /// The UI takes responsibility for placement or ending this terminal.
    /// Acknowledging never waits: the one-slot channel is empty until here.
    pub(super) fn take(self) -> Result<TerminalInfo, super::Error> {
        if let Some(received) = self.received {
            let _ = received.send(());
        }
        self.result
    }

    #[cfg(test)]
    pub(super) fn unwatched(result: Result<TerminalInfo, super::Error>) -> Self {
        Self {
            result,
            received: None,
        }
    }
}

pub(super) fn deliver(
    result: Result<TerminalInfo, super::Error>,
    sender: SyncSender<Outcome>,
    end: impl FnOnce(&TerminalId),
) -> bool {
    let created = result.as_ref().ok().map(|info| info.id.clone());
    let (received, accepted) = mpsc::sync_channel(1);
    let handed = sender
        .send(Outcome {
            result,
            received: Some(received),
        })
        .is_ok()
        && accepted.recv().is_ok();
    if !handed && let Some(terminal) = created {
        end(&terminal);
    }
    handed
}

impl crate::SheprdWindow {
    /// Starts the wanted shell once the hub is connected.
    pub(super) fn start_core_shell(&mut self) {
        if self.core_terminals.starting.is_some() {
            return;
        }
        let terminals = &mut self.core_terminals;
        // A disconnected host must not block starts on another ready backend.
        let Some((index, backend)) =
            terminals
                .wanted
                .iter()
                .enumerate()
                .find_map(|(index, wanted)| {
                    terminals
                        .provider(&wanted.host, wanted.backend, self.config.herdr_2_scrollback)
                        .map(|backend| (index, backend))
                })
        else {
            return;
        };
        let Some(Wanted {
            purpose,
            host,
            spawn,
            file,
            ..
        }) = terminals.wanted.remove(index)
        else {
            return;
        };
        let (send, receive) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("herdr-core-spawn".into())
            .spawn(move || {
                let started = backend.spawn(&spawn);
                let succeeded = started.is_ok();
                let handed = deliver(started, send, |terminal| {
                    // The receiver vanished with its host/window. End only
                    // the terminal this worker created, on its captured core;
                    // never retry its ID against a replacement saved host.
                    if let Err(error) = backend.kill(terminal) {
                        tracing::warn!(%error, %terminal, "Could not end an abandoned herdr-core start");
                    }
                });
                // Keep a program's handoff file only after its window accepts
                // the start; failed/cancelled starts retain local cleanup.
                if handed && succeeded
                    && let Some(file) = file
                    && let Err(error) = file.keep() {
                    tracing::warn!(%error, "Could not hand a file to a herdr-core program");
                }
            });
        match spawned {
            Ok(_) => terminals.starting = Some((purpose, host, receive)),
            Err(error) => tracing::warn!(%error, "Could not start a herdr-core spawn worker"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
