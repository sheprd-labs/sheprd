//! The kept workspaces and herdr-core tabs every client of a core shares,
//! held in that core's `sheprd.spaces` document (`docs/shared-spaces-plan.md`).
//!
//! For each core a window reaches, its document is read every couple of
//! seconds on a worker thread and mirrored into the store's kept
//! workspaces and tabs of that host's scope, so another computer's or a
//! phone's workspace shows here too. Every change the app makes there is
//! still made in the store at once, as before; it is then told from the
//! document as last read and written as `spaces::Edit`s, one job at a time
//! per core. The first read this run adopts what the app kept before the
//! document held it.
//!
//! The state is the app's, not a window's: every window reads the one
//! store, so whichever window ticks first moves a core's sync along.

mod diff;
mod migrate;

pub(crate) use migrate::{Offer, migrate};

/// The edits from `base` to `local`, for the store's tests.
#[cfg(test)]
pub(crate) fn edits_for_test(base: &View, local: &View) -> Vec<Edit> {
    diff::edits(base, local)
}

use super::Host;
use crate::{
    SheprdWindow,
    browser::{
        Scope, Store,
        shared::{Pending, Record, TAB_PREFIX, View, new_id},
    },
    i18n::fl,
    window::Flash,
};
use gpui::{App, Context, Global};
use sheprd_backend::TerminalId;
use sheprd_core_client::{Core, CoreBackend};
use sheprd_session::spaces::{Adopted, Edit, Spaces};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{Receiver, TryRecvError, sync_channel},
    time::{Duration, Instant},
};

/// How often a core's document is read again while it answers.
const READ_EVERY: Duration = Duration::from_secs(2);
/// How often the store is looked at for changes to write.
const LOOK_EVERY: Duration = Duration::from_millis(200);
/// How long a core that failed to answer waits before the next job.
const RETRY: Duration = Duration::from_secs(2);

/// What a job does with a core's document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Job {
    /// The first read this run, adopting what the app kept.
    Migrate(Offer),
    Read,
    Update(Vec<Edit>),
}

/// The document a job leaves, its version, and the ids a migration took.
#[derive(Debug)]
pub(crate) struct Answer {
    pub(crate) adopted: Option<Adopted>,
    /// Whether the document was written anew since the app last read it,
    /// so what it held then says nothing of it.
    pub(crate) reset: bool,
    pub(crate) spaces: Spaces,
    pub(crate) version: u64,
}

pub(crate) type Outcome = Result<Answer, sheprd_core_client::Error>;

/// Which job is on its way, and what it was sent with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sent {
    /// The view offered, against which changes made since are told.
    Migrate(View),
    Read,
    /// The view the edits were made from, put back if they never arrived.
    Update(View),
}

/// One core's sync.
#[derive(Debug)]
pub(crate) struct CoreSync {
    /// The scope whose kept workspaces mirror the document.
    scope: Scope,
    /// The scope's view as the document held it when last read, which
    /// changes here are told against; none until the first read this run.
    base: Option<View>,
    /// Whether the first job this run was answered, even with an error.
    answered: bool,
    failed: Option<Instant>,
    /// Terminals the document's tabs hold, which adoption leaves alone.
    held: HashSet<TerminalId>,
    looked: Option<Instant>,
    read: Option<Instant>,
    /// Why the core refused a change, to show once.
    refused: Option<String>,
    /// A layout written over a tab another client changed: the document
    /// is read before the change is made again.
    reread: bool,
    /// A refused change, and the view its edits were made from: the next
    /// answer is taken over what those edits changed, keeping what changed
    /// here since.
    revert: Option<View>,
    /// Records the last answer held that the store had no room for, so a
    /// read of the same document tries them again.
    incomplete: bool,
    sent: Option<Sent>,
    version: u64,
    /// Why this version may not write the document, when it may not.
    newer: Option<String>,
    /// Whether this version may write the document.
    writable: bool,
}

impl CoreSync {
    pub(crate) fn new(scope: Scope) -> Self {
        Self {
            scope,
            base: None,
            answered: false,
            failed: None,
            held: HashSet::new(),
            looked: None,
            read: None,
            refused: None,
            reread: false,
            revert: None,
            incomplete: false,
            sent: None,
            version: 0,
            newer: None,
            writable: true,
        }
    }

    /// Whether a tab of the document holds `terminal`.
    pub(crate) fn holds(&self, terminal: &TerminalId) -> bool {
        self.held.contains(terminal)
    }

    /// Why the core last refused a change, once.
    pub(crate) fn take_refused(&mut self) -> Option<String> {
        self.refused.take()
    }

    /// Whether a job may be sent now: none is on its way, a core that did
    /// not answer has waited, and one is due. Read without the store, so a
    /// tick with nothing to do claims nothing.
    pub(crate) fn due(&self, now: Instant) -> bool {
        let since = |at: Option<Instant>, every| {
            at.is_none_or(|at| now.saturating_duration_since(at) >= every)
        };
        self.sent.is_none()
            && since(self.failed, RETRY)
            && (self.base.is_none()
                || self.reread
                || since(self.looked, LOOK_EVERY)
                || since(self.read, READ_EVERY))
    }

    /// The job to send now, if one is due and none is on its way. Every
    /// shared tab has its document id already ([`Store::name_shared_tabs`]).
    pub(crate) fn next(&mut self, store: &Store, now: Instant) -> Option<Job> {
        if !self.due(now) {
            return None;
        }
        let Some(base) = &self.base else {
            let view = store.shared_view(&self.scope);
            let synced = store.synced(&self.scope).cloned().unwrap_or_default();
            self.sent = Some(Sent::Migrate(view.clone()));
            return Some(Job::Migrate(Offer { synced, view }));
        };
        if std::mem::take(&mut self.reread) {
            self.read = Some(now);
            self.sent = Some(Sent::Read);
            return Some(Job::Read);
        }
        if (self.looked).is_none_or(|looked| now.saturating_duration_since(looked) >= LOOK_EVERY) {
            self.looked = Some(now);
            let local = store.shared_view(&self.scope);
            let edits = diff::edits(base, &local);
            if !edits.is_empty() {
                if !self.writable {
                    // Shown once; the document stays as the newer
                    // version wrote it.
                    self.refused = self.newer.clone();
                    self.base = Some(local);
                    return None;
                }
                let from = base.clone();
                self.base = Some(local);
                self.sent = Some(Sent::Update(from));
                return Some(Job::Update(edits));
            }
        }
        if (self.read).is_none_or(|read| now.saturating_duration_since(read) >= READ_EVERY) {
            self.read = Some(now);
            self.sent = Some(Sent::Read);
            return Some(Job::Read);
        }
        None
    }

    /// Takes a read that found the document as last read, which needs
    /// nothing of the store; false for any other outcome, left for
    /// [`Self::finish`].
    pub(crate) fn finish_quiet(&mut self, outcome: &Outcome) -> bool {
        let quiet = self.sent == Some(Sent::Read)
            && self.revert.is_none()
            && !self.incomplete
            && self.base.is_some()
            && outcome
                .as_ref()
                .is_ok_and(|answer| answer.version == self.version);
        if quiet {
            self.sent = None;
            self.failed = None;
        }
        quiet
    }

    /// The job just taken from [`Self::next`] could not be sent: what it
    /// would have written is told again once the core may be asked.
    pub(crate) fn unsent(&mut self, now: Instant) {
        if let Some(Sent::Update(from)) = self.sent.take() {
            self.base = Some(from);
        }
        self.failed = Some(now);
    }

    /// Whether the document was read this run, or could not be: until then
    /// it may hold any terminal.
    pub(crate) fn answered(&self) -> bool {
        self.answered
    }

    /// Takes the outcome of the job on its way. Whether the store changed.
    pub(crate) fn finish(&mut self, store: &mut Store, outcome: Outcome, now: Instant) -> bool {
        let Some(sent) = self.sent.take() else {
            return false;
        };
        self.answered = true;
        let answer = match outcome {
            Ok(answer) => answer,
            Err(sheprd_core_client::Error::Spaces(sheprd_session::spaces::Error::Stale(tab))) => {
                // Another client opened or closed a pane of the tab since
                // it was read: read it again, and the change is made again
                // to what that client left.
                tracing::debug!(%tab, "A shared tab changed under a layout edit; redoing it");
                if let Sent::Update(from) = sent {
                    self.base = Some(from);
                }
                self.reread = true;
                return false;
            }
            Err(error) if refused(&error) => {
                tracing::warn!(%error, "herdr-core refused a shared workspace change");
                self.refused = Some(error.to_string());
                if let Sent::Update(from) = sent {
                    // The document stays as it was, and so does what the
                    // refused edits changed here once it is read again.
                    self.revert = Some(from);
                    self.read = None;
                } else {
                    self.failed = Some(now);
                }
                return false;
            }
            Err(error) => {
                tracing::debug!(%error, "Could not reach a herdr-core's shared workspaces");
                self.failed = Some(now);
                if let Sent::Update(from) = sent {
                    self.base = Some(from);
                }
                return false;
            }
        };
        self.failed = None;
        if self.base.is_some() && answer.version < self.version {
            // A new document, as when the core's state was wiped: what the
            // app holds is offered to it as at first.
            self.base = None;
            self.version = 0;
            store.set_synced(&self.scope, None);
            return false;
        }
        let revert = self.revert.take();
        if revert.is_none()
            && !self.incomplete
            && answer.version == self.version
            && !matches!(sent, Sent::Migrate(_))
        {
            return false;
        }
        self.writable = answer.spaces.writable();
        self.newer = (answer.spaces.to_value().err()).map(|error| error.to_string());
        self.held = (answer.spaces.tabs().iter())
            .flat_map(|tab| tab.panes.terminals().into_iter().cloned())
            .collect();
        let document = View::of(&answer.spaces);
        // What the document held as this app last knew it, and, on the
        // first read, what it held before the app last quit.
        let (before, synced) = match sent {
            Sent::Migrate(offered) => {
                let before = match &answer.adopted {
                    Some(adopted) => {
                        store.take_adopted(&self.scope, adopted);
                        offered.renamed(adopted)
                    }
                    // Not adopted: nothing offered is the document's.
                    None => View::default(),
                };
                let synced = (store.synced(&self.scope).cloned())
                    .filter(|_| !answer.reset)
                    .unwrap_or_default();
                (before, synced)
            }
            Sent::Read | Sent::Update(_) => {
                (self.base.take().unwrap_or_default(), Default::default())
            }
        };
        let local = store.shared_view(&self.scope);
        // After a refusal `before` is the view the refused edits made, so
        // only what changed here since stays pending.
        let pending = Pending::of(&diff::edits(&before, &local));
        let changed = store.mirror_shared(&self.scope, &document, &pending, |record| {
            before.has(record)
                || revert.as_ref().is_some_and(|from| from.has(record))
                || match record {
                    Record::Tab(id) => synced.tabs.contains(id),
                    Record::Workspace(id) => synced.workspaces.contains(id),
                }
        });
        self.version = answer.version;
        // The document's records the store holds, and those removed here
        // and not written yet, so the next edits are told against it.
        let holding = store.shared_view(&self.scope);
        let base = (document.clone()).only(|record| holding.has(record) || pending.removed(record));
        self.incomplete = (document.workspaces.iter())
            .map(|workspace| Record::Workspace(&workspace.id))
            .chain(document.tabs.iter().map(|tab| Record::Tab(&tab.id)))
            .any(|record| !base.has(record));
        store.set_synced(&self.scope, Some(base.synced(self.version)));
        self.base = Some(base);
        changed
    }
}

/// Whether herdr-core refused a change for what it is, rather than failing
/// to answer: a bound, a newer format, or a record another client removed.
fn refused(error: &sheprd_core_client::Error) -> bool {
    matches!(
        error,
        sheprd_core_client::Error::Spaces(_) | sheprd_core_client::Error::Refused { .. }
    )
}

/// Runs `job` against `core`. Blocking: on a worker thread only.
pub(crate) fn run(core: &Core, job: Job) -> Outcome {
    use sheprd_core_client::spaces;
    match job {
        Job::Read => spaces::read(core).map(|(spaces, version)| Answer {
            adopted: None,
            reset: false,
            spaces,
            version,
        }),
        Job::Update(edits) => spaces::update(core, &edits).map(|(spaces, version)| Answer {
            adopted: None,
            reset: false,
            spaces,
            version,
        }),
        Job::Migrate(offer) => {
            let (current, version) = spaces::read(core)?;
            let (offer, reset) = offer.at(version);
            if !current.writable() {
                return Ok(Answer {
                    adopted: None,
                    reset,
                    spaces: current,
                    version,
                });
            }
            let mut adopted = Adopted::default();
            let (spaces, version) = spaces::change(core, |spaces| {
                adopted = migrate(spaces, &offer)?;
                Ok(())
            })?;
            Ok(Answer {
                adopted: Some(adopted),
                reset,
                spaces,
                version,
            })
        }
    }
}

/// Each core's sync, and the answer on its way.
#[derive(Default)]
pub(crate) struct Shared {
    hosts: HashMap<Host, (CoreSync, Option<Receiver<Outcome>>)>,
}

impl Global for Shared {}

impl Shared {
    /// Whether a tab of `host`'s document may hold `terminal`: one does,
    /// or the document has not been read yet this run. A core the window
    /// does not sync holds none.
    pub(crate) fn may_hold(cx: &App, host: &Host, terminal: &TerminalId) -> bool {
        cx.try_global::<Self>()
            .and_then(|shared| shared.hosts.get(host))
            .is_some_and(|(sync, _)| !sync.answered() || sync.holds(terminal))
    }
}

impl SheprdWindow {
    /// Moves each reached core's sync along: takes a finished job's answer
    /// and sends the next job on a worker thread.
    pub(super) fn sync_shared_spaces(&mut self, cx: &mut Context<Self>) {
        let local = self
            .core_host()
            .map(|index| crate::browser::scope(&self.endpoints[index]));
        let hosts: Vec<(Host, Scope, CoreBackend)> = (self.core_terminals.hosts().into_iter())
            .filter_map(|host| {
                let backend = self.core_terminals.backend_of(&host)?.clone();
                let scope = match &host {
                    Host::Local => local.clone()?,
                    Host::Remote(scope) => scope.clone(),
                };
                Some((host, scope, backend))
            })
            .collect();
        if hosts.is_empty() || !cx.has_global::<Store>() {
            return;
        }
        if !cx.has_global::<Shared>() {
            cx.set_global(Shared::default());
        }
        let now = Instant::now();
        let mut changed = false;
        for (host, scope, backend) in hosts {
            let (mut sync, mut pending) = (cx.global_mut::<Shared>().hosts.remove(&host))
                .unwrap_or_else(|| (CoreSync::new(scope.clone()), None));
            // Another window's local scope syncs this core already.
            if sync.scope == scope {
                let finished = match pending.as_ref().map(Receiver::try_recv) {
                    Some(Ok(outcome)) => Some(outcome),
                    Some(Err(TryRecvError::Disconnected)) => {
                        Some(Err(sheprd_core_client::Error::Lost))
                    }
                    Some(Err(TryRecvError::Empty)) | None => None,
                };
                if let Some(outcome) = finished {
                    pending = None;
                    if !sync.finish_quiet(&outcome) {
                        changed |= Store::update(cx, |store| sync.finish(store, outcome, now));
                    }
                }
                if pending.is_none() && sync.due(now) {
                    // Claiming the store redraws every window: only for a
                    // tab that needs its document id.
                    if cx.global::<Store>().unnamed_shared_tabs(&scope) {
                        Store::update(cx, |store| {
                            store.name_shared_tabs(&scope, || new_id(TAB_PREFIX))
                        });
                    }
                    if let Some(job) = sync.next(cx.global::<Store>(), now) {
                        pending = send(backend, job);
                        if pending.is_none() {
                            sync.unsent(now);
                        }
                    }
                }
                if let Some(reason) = sync.take_refused() {
                    self.show_flash(
                        Flash::warning(fl!("core-terminals-shared-refused", reason = reason)),
                        cx,
                    );
                }
            }
            cx.global_mut::<Shared>()
                .hosts
                .insert(host, (sync, pending));
        }
        if changed {
            self.sync_core_views(cx);
            cx.notify();
        }
    }
}

/// Runs `job` on a worker thread of its own, as the window's other
/// herdr-core calls do.
fn send(backend: CoreBackend, job: Job) -> Option<Receiver<Outcome>> {
    let (answer, answered) = sync_channel(1);
    let started = std::thread::Builder::new()
        .name("herdr-core-spaces".into())
        .spawn(move || {
            let _ = answer.send(run(backend.core(), job));
        });
    match started {
        Ok(_) => Some(answered),
        Err(error) => {
            tracing::warn!(%error, "Could not start a herdr-core shared workspaces worker");
            None
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
