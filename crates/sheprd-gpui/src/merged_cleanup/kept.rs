//! Merged cleanup of the linked checkouts of workspaces the app keeps for
//! Herdr 2. No daemon keeps them, so they are found in the app's own
//! session and checked as Herdr 1's are: the focused one asks through the
//! kept delete dialog, and any other is removed through
//! `sheprd_remote::work` without force, so a checkout that gained changes
//! after the check stays. A removed checkout's kept workspace closes.

use super::{ASK_DEFER, Action, Candidate, Ledger, Probe, RETRY, SCAN, decide_status, probe_on};
use crate::{
    SheprdWindow,
    pull_request::{Input, PullRequest, State},
};
use gpui::{Context, Window};
use sheprd_session::{Keeper, Workspace};
use std::{
    sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::{self, TryRecvError},
    },
    time::Instant,
};

/// The cleanup's state for kept workspaces, which outlives Herdr 1's
/// connection: their IDs are the app's own.
#[derive(Default)]
pub(crate) struct Kept {
    target: Option<sheprd_client::ConnectTarget>,
    probe: Option<Probe>,
    ledger: Ledger,
    next_scan: Option<Instant>,
    /// The last scan found nothing to check.
    pub(super) idle: bool,
    /// Removals sent and not yet answered, by request ID.
    removing: Vec<(String, Candidate)>,
}

impl Kept {
    /// Forgets earlier refusals, so a sweep checks every candidate again.
    pub(super) fn reset_for_sweep(&mut self) {
        self.ledger = Ledger::default();
        self.next_scan = None;
        self.idle = false;
    }

    /// The kept checkout being checked now.
    pub(super) fn current(&self) -> Option<&Candidate> {
        self.probe.as_ref().map(|probe| &probe.candidate)
    }

    /// Waits for the answer to the removal sent as `request`.
    pub(super) fn track_removal(&mut self, request: String, candidate: Candidate) {
        self.removing.push((request, candidate));
    }

    /// Nothing left to check, and no check or removal running.
    pub(super) fn settled(&self) -> bool {
        self.idle && self.probe.is_none() && self.removing.is_empty()
    }
}

/// The kept linked checkouts among `workspaces` whose cached pull request is
/// merged, in session order. A main checkout never qualifies.
#[cfg(test)]
pub(crate) fn kept_candidates<'a, R: std::ops::Deref<Target = PullRequest>>(
    workspaces: &'a [Workspace],
    pull_request: impl Fn(&Input) -> Option<R> + 'a,
) -> impl Iterator<Item = Candidate> + 'a {
    kept_candidates_on(workspaces, false, pull_request)
}

pub(super) fn kept_candidates_on<'a, R: std::ops::Deref<Target = PullRequest>>(
    workspaces: &'a [Workspace],
    remote: bool,
    pull_request: impl Fn(&Input) -> Option<R> + 'a,
) -> impl Iterator<Item = Candidate> + 'a {
    workspaces.iter().filter_map(move |workspace| {
        workspace
            .worktree
            .as_ref()
            .filter(|tree| tree.linked && workspace.keeper == Keeper::App)?;
        let mut input = crate::pull_request::kept_input(workspace, remote)?;
        let pr = pull_request(&input).filter(|pr| pr.state == State::Merged)?;
        // The PR cache is keyed by repository and branch, but the probe
        // must inspect this workspace's folder, not another checkout now
        // holding its cached branch.
        if workspace.cwd.is_empty() {
            return None;
        }
        input.checkout = Some(workspace.cwd.clone());
        Some(Candidate {
            workspace: workspace.id.clone(),
            number: pr.number,
            head: pr.head_ref_oid.clone(),
            input,
        })
    })
}

impl SheprdWindow {
    /// The selected host's kept workspaces, when it is this computer.
    fn kept_workspaces(&self) -> Vec<Workspace> {
        self.sessions(self.selected_endpoint)
            .core
            .map(|core| core.workspaces.clone())
            .unwrap_or_default()
    }

    /// Scans kept workspaces for a merged pull request and checks one at a
    /// time off the UI thread, as Herdr 1's are.
    pub(super) fn update_kept_merged_cleanup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.endpoints[self.selected_endpoint]
            .connection
            .target
            .clone();
        if self.merged_cleanup.kept.target.as_ref() != Some(&target) {
            self.merged_cleanup.kept = Kept {
                target: Some(target.clone()),
                ..Kept::default()
            };
        }
        if self.merged_cleanup.kept.probe.is_some() {
            self.finish_kept_merged_probe(window, cx);
            return;
        }
        let now = Instant::now();
        if self
            .merged_cleanup
            .kept
            .next_scan
            .is_some_and(|next| now < next)
        {
            return;
        }
        let interval = if self.merged_cleanup.sweep.is_some() {
            std::time::Duration::ZERO
        } else {
            SCAN
        };
        self.merged_cleanup.kept.next_scan = Some(now + interval);
        let workspaces = self.kept_workspaces();
        let Some(view) = self.endpoint_pr_view(self.selected_endpoint) else {
            self.merged_cleanup.kept.idle = true;
            return;
        };
        let cache = &self.menu.pr_cache;
        let cleanup = &self.merged_cleanup;
        let found = kept_candidates_on(&workspaces, self.selected_is_remote(), |input| {
            cache.peek_input(&view, input)
        })
        .find(|candidate| {
            !cleanup.kept.ledger.waiting(candidate, now) && !cleanup.handled(candidate)
        });
        self.merged_cleanup.kept.idle = found.is_none();
        let Some(candidate) = found else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let (input, cancellation) = (candidate.input.clone(), cancelled.clone());
        cx.background_executor()
            .spawn(async move {
                let _ = sender.send(probe_on(&target, &input, &cancellation));
            })
            .detach();
        self.merged_cleanup.kept.probe = Some(Probe {
            candidate,
            receiver,
            cancelled,
        });
    }

    fn finish_kept_merged_probe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = match self
            .merged_cleanup
            .kept
            .probe
            .as_ref()
            .map(|probe| probe.receiver.try_recv())
        {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Empty)) | None => return,
            Some(Err(TryRecvError::Disconnected)) => Err(crate::Error::StaleWorkspace),
        };
        let Some(probe) = self.merged_cleanup.kept.probe.take() else {
            return;
        };
        let candidate = probe.candidate.clone();
        let now = Instant::now();
        let retry = Some(now + RETRY);
        // Rechecked against what is kept now: the workspace may have closed
        // or changed branch while Git ran.
        let workspaces = self.kept_workspaces();
        let Some(view) = self.endpoint_pr_view(self.selected_endpoint) else {
            return;
        };
        let cache = &self.menu.pr_cache;
        let current = kept_candidates_on(&workspaces, self.selected_is_remote(), |input| {
            cache.peek_input(&view, input)
        })
        .any(|c| c == candidate);
        let workspace = workspaces
            .iter()
            .find(|workspace| workspace.id == candidate.workspace);
        let (Some(workspace), true) = (workspace, current) else {
            return;
        };
        let Ok(report) = result else {
            // Git's diagnostics may name private paths; log the category only.
            tracing::debug!(category = "check_failed", "Merged kept worktree kept");
            self.merged_cleanup.skipped(&candidate);
            self.merged_cleanup.kept.ledger.settle(candidate, retry);
            return;
        };
        if self.menu.open_on(&candidate.workspace) {
            self.merged_cleanup.kept.next_scan = Some(now + ASK_DEFER);
            return;
        }
        // A sweep was confirmed already, so even the one in front goes.
        let focused = self.kept_in_front() == Some(candidate.workspace.as_str())
            && self.merged_cleanup.sweep.is_none();
        match decide_status(&candidate, &report, workspace.status, focused) {
            Err(skip) => {
                tracing::debug!(?skip, "Merged kept worktree kept");
                self.merged_cleanup.skipped(&candidate);
                self.merged_cleanup.kept.ledger.settle(candidate, retry);
            }
            Ok(Action::Ask) => {
                // Never interrupt an open menu or dialog; ask once it closes.
                if self.menu.page.is_some() {
                    self.merged_cleanup.kept.next_scan = Some(now + ASK_DEFER);
                    return;
                }
                self.ask_merged_removal(&candidate.workspace, candidate.number, window, cx);
                self.merged_cleanup.kept.ledger.settle(candidate, None);
            }
            Ok(Action::Remove) => {
                let (id, path) = (workspace.id.clone(), workspace.cwd.clone());
                // A removal that is refused, such as for files that appeared
                // after the check, is tried again later, never with force.
                self.merged_cleanup
                    .kept
                    .ledger
                    .settle(candidate.clone(), retry);
                // Counted once Git answers, which the sweep waits for.
                self.merged_cleanup.handle(&candidate);
                let request = self.remove_merged_kept_checkout(id, path, candidate.clone(), cx);
                self.merged_cleanup.kept.track_removal(request, candidate);
            }
        }
        cx.notify();
    }

    /// The answer to a kept checkout's removal: a cleanup's removal counts
    /// toward a running sweep, removed or kept, once Git has answered.
    pub(crate) fn finish_merged_kept_removal(&mut self, request: &str, removed: bool) {
        let kept = &mut self.merged_cleanup.kept;
        let Some(at) = kept.removing.iter().position(|(id, _)| id == request) else {
            return;
        };
        let (_, candidate) = kept.removing.remove(at);
        if removed {
            self.merged_cleanup.removed(&candidate);
        } else {
            self.merged_cleanup.skipped(&candidate);
        }
    }
}
