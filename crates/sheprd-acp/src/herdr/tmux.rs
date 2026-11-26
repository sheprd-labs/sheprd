//! Publishes the chat's own metadata through the same coalesced mailbox as
//! Herdr reports. It runs even while the desktop is detached.
use super::*;
use sheprd_client::protocol::AgentStatus;
use sheprd_tmux::{AgentReport, Tmux};

impl Reporter {
    pub(crate) fn on_tmux(display: String, endpoint: Option<String>) -> Result<Option<Self>> {
        let Some(id) = Tmux::caller() else {
            return Ok(None);
        };
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let shared = mailbox.clone();
        let handle = thread::Builder::new()
            .name("tmux-report".into())
            .spawn(move || {
                let backend = Tmux::new(0);
                let mut report = AgentReport {
                    pid: std::process::id(),
                    display,
                    endpoint,
                    status: AgentStatus::Working,
                    title: None,
                };
                let (lock, wake) = &*shared;
                loop {
                    let result = backend.report(&id, Some(&report));
                    let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
                    while !state.closing
                        && state.activity.is_none()
                        && state.prompt.is_none()
                        && state.display.is_none()
                    {
                        if result.is_err() {
                            let (next, waited) = wake
                                .wait_timeout(state, RETRY)
                                .unwrap_or_else(PoisonError::into_inner);
                            state = next;
                            if waited.timed_out() {
                                break;
                            }
                        } else {
                            state = wake.wait(state).unwrap_or_else(PoisonError::into_inner);
                        }
                    }
                    if state.closing {
                        drop(state);
                        let _ = backend.report(&id, None);
                        return;
                    }
                    if let Some(display) = state.display.take() {
                        report.display = display;
                    }
                    if let Some(prompt) = state.prompt.take() {
                        report.title = naming::title(&prompt);
                    }
                    if let Some(activity) = state.activity.take() {
                        report.status = match activity {
                            Activity::Idle => AgentStatus::Idle,
                            Activity::Working => AgentStatus::Working,
                            Activity::Blocked => AgentStatus::Blocked,
                        };
                    }
                }
            })?;
        Ok(Some(Self {
            handle: Some(handle),
            mailbox,
        }))
    }
}
