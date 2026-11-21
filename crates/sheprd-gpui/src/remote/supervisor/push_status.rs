//! How the daemon's iPhone pushes leave it (`Request::PushStatus`), for
//! Settings' push relay row: asked with each poll while a lease is held.
use super::Supervisor;
use gpui::App;
use sheprd_remote_control::control::{self, Response, push::PushStatus};

impl Supervisor {
    /// Takes a poll's answer. Returns whether what Settings shows changed.
    /// A daemon too old to answer leaves it unknown.
    pub(super) fn push_polled(&mut self, answer: Result<Response, control::Error>) -> bool {
        let status = match answer {
            Ok(Response::PushStatus(status)) => Some(status),
            _ => None,
        };
        let changed = status != self.push_status;
        self.push_status = status;
        changed
    }

    pub(crate) fn push_status(cx: &App) -> Option<&PushStatus> {
        cx.try_global::<Self>()?.push_status.as_ref()
    }

    /// Takes a push status as a poll would have.
    #[cfg(test)]
    pub(crate) fn set_push_status(cx: &mut App, status: Option<PushStatus>) {
        cx.global_mut::<Self>().push_status = status;
    }
}
