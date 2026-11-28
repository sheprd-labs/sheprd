//! What a Live Activity may say on one device. [`crate::apns::decide`]
//! writes everything the turn has; this removes what the user chose to keep
//! off the lock screen, and off anything a push passes through.
//!
//! | | Detailed | Generic |
//! | --- | --- | --- |
//! | agent, phase, plan counts, elapsed time | yes | yes |
//! | chat title, workspace, steps | yes | no |
//! | usage summary (`Worked 12s`) | yes | no |
//! | question, its buttons, failure message | with `Privacy::Excerpt` | no |
//!
//! Buttons go with the question: nobody should answer from the lock screen
//! without seeing what they are answering, and the app checks an answer
//! against the question shown (`ActivityAnswer`).

use super::{Detail, Event, Live, Phase};
use crate::{app::config::ActivityDetail, push::Privacy};

/// The text of an alert that may not name the chat.
const GENERIC_BODY: &str = "Open Sheprd for details";
/// What a generic activity shows in place of the question.
const GENERIC_QUESTION: &str = "Open Sheprd to answer";

impl Live {
    /// This push with only what `privacy` and `detail` allow.
    pub(crate) fn redacted(mut self, privacy: Privacy, detail: ActivityDetail) -> Self {
        if privacy == Privacy::Generic || detail == ActivityDetail::Generic {
            self.hide_quotes();
        }
        if detail == ActivityDetail::Generic {
            self.hide_chat();
        }
        self
    }

    /// Removes text that may quote the chat.
    fn hide_quotes(&mut self) {
        let state = &mut self.state;
        if let Some(agents) = &mut state.detail.agents {
            agents.latest = None;
        }
        state.question = None;
        state.permission = None;
        state.options.clear();
        state.detail.blocker_title = None;
        if state.phase == Phase::Failed {
            state.summary = None;
        }
    }

    /// Removes everything that says which chat this is, or what it does.
    fn hide_chat(&mut self) {
        let state = &mut self.state;
        state.previous.clear();
        state.summary = None;
        // Words, icon, and counts all tell what the turn does.
        let focus = state.detail.focus.take().map(|mut focus| {
            focus.chat_title.clear();
            focus.workspace.clone_from(&focus.computer);
            focus
        });
        state.detail = Detail {
            question_revision: state.detail.question_revision,
            focus,
            agents: state.detail.agents.take(),
            ..Detail::default()
        };
        state.step = match state.phase {
            Phase::Working => super::decide::DEFAULT_STEP,
            Phase::Question => GENERIC_QUESTION,
            Phase::Failed | Phase::Finished => GENERIC_BODY,
        }
        .to_owned();
        match &mut self.event {
            Event::Start { alert, attributes } => {
                alert.body = GENERIC_BODY.to_owned();
                attributes.chat_title.clear();
                // The banner's place line: the computer still tells several
                // apart, and it is the user's own name for it.
                attributes.workspace.clone_from(&attributes.computer);
            }
            Event::Update { alert: Some(alert) } => alert.body = GENERIC_BODY.to_owned(),
            Event::Update { alert: None } | Event::End { .. } => {}
        }
    }
}
