//! What a terminal agent says it did, read off its screen once its run
//! finished, away from the notifier's loop: the finish waits at most
//! [`WAIT`] for the read, then goes out as it was decided.

use crate::{
    native::previews::preview,
    push::{Notification, recapped},
    web::State,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

/// How long a finish waits for the agent's screen.
const WAIT: Duration = Duration::from_secs(2);

/// Sends `finished` on `told` with what its pane's screen says the agent
/// did, or as it is when the screen is slow, unreadable, or says nothing.
pub(super) fn tell(
    state: &Arc<State>,
    finished: Notification,
    task: Option<String>,
    told: mpsc::UnboundedSender<Notification>,
) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let screen = tokio::time::timeout(WAIT, state.hub.screen_text(&finished.pane)).await;
        let recap = match screen {
            Ok(Ok(screen)) => preview(&screen),
            Ok(Err(error)) => {
                tracing::debug!(%error, "Could not read a finished agent's screen");
                None
            }
            Err(_) => None,
        };
        let finished = match recap {
            Some(recap) => recapped(finished, task.as_deref(), &recap),
            None => finished,
        };
        let _ = told.send(finished);
    });
}
