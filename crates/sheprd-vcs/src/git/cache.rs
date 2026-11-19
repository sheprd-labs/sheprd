//! The bounded answers the sidebar reads for every listed checkout.

use super::{Answer, Counted, Git, Input, Instant, Status};

impl Git {
    /// Preserve the last focused status when the user switches workspaces.
    /// Listed workspaces refresh more slowly, so this keeps their counts
    /// visible while their next probe is pending.
    pub(super) fn keep_focused_status(&mut self, now: Instant) {
        let (Some(input), Some(lines)) = (self.input.clone(), self.status) else {
            return;
        };
        let dirty = self.uncommitted.unwrap_or_else(|| lines.dirty());
        let answer = if self.counted {
            Answer::Counted(Counted { lines, dirty })
        } else {
            Answer::Dirty(dirty)
        };
        self.record_probe(input, Some(answer), now);
    }

    /// A checkout's cached dirty bit by its full input. Unlike [`Self::dirty`],
    /// this also finds a local workspace Git resolved from its directory when
    /// the daemon has no repository key for it.
    pub fn dirty_for_input(&self, input: &Input) -> Option<bool> {
        if self.input.as_ref() == Some(input)
            && let Some(uncommitted) = self.uncommitted
        {
            return Some(uncommitted);
        }
        self.probes
            .iter()
            .find(|probe| probe.input == *input)
            .and_then(|probe| probe.answer)
            .map(Answer::dirty)
    }

    /// A checkout's cached line counts by its full input, including local
    /// workspaces identified only by their directory and branch.
    pub fn counts_for_input(&self, input: &Input) -> Option<Status> {
        if self.input.as_ref() == Some(input)
            && let Some(status) = self.status
        {
            return Some(status);
        }
        self.probes
            .iter()
            .find(|probe| probe.input == *input)
            .and_then(|probe| match probe.answer {
                Some(Answer::Counted(counted)) => Some(counted.lines),
                _ => None,
            })
    }
}
