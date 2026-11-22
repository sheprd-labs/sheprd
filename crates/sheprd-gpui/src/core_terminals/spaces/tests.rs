use super::*;
use crate::browser::Location;
use crate::core_terminals::panes::Panes;
use serde_json::Value;

mod diff;
mod merge;
mod migration;
mod refused;
mod two_computers;

/// A core's document as herdr-core keeps it: a value and its version, each
/// job applied to the document as it is when the job arrives, as
/// `sheprd_core_client::spaces` does after a conflict.
#[derive(Default)]
struct FakeCore {
    value: Value,
    version: u64,
    writes: u64,
}

impl FakeCore {
    fn spaces(&self) -> Spaces {
        Spaces::from_value(self.value.clone()).unwrap()
    }

    fn run(&mut self, job: Job) -> Outcome {
        let read = self.spaces();
        let (mut spaces, mut adopted, mut reset) = (read.clone(), None, false);
        match job {
            Job::Read => {}
            Job::Update(edits) => {
                for edit in edits {
                    spaces.apply(edit)?;
                }
            }
            Job::Migrate(offer) => {
                let (offer, wiped) = offer.at(self.version);
                reset = wiped;
                if read.writable() {
                    adopted = Some(migrate(&mut spaces, &offer)?);
                }
            }
        }
        if spaces != read {
            self.value = spaces.to_value()?;
            self.version += 1;
            self.writes += 1;
        }
        Ok(Answer {
            adopted,
            reset,
            spaces: self.spaces(),
            version: self.version,
        })
    }
}

/// One computer showing the host: its store and its sync of the core.
struct Client {
    now: Instant,
    store: Store,
    sync: CoreSync,
}

impl Client {
    fn new() -> Self {
        Self {
            now: Instant::now(),
            store: Store::default(),
            sync: CoreSync::new(scope()),
        }
    }

    /// The app restarting: the same saved tabs, a new sync.
    fn restart(&mut self) {
        self.sync = CoreSync::new(scope());
    }

    /// Takes the next job, past every wait, without running it.
    fn send(&mut self) -> Option<Job> {
        self.now += Duration::from_secs(3);
        self.store.name_shared_tabs(&scope(), || new_id(TAB_PREFIX));
        self.sync.next(&self.store, self.now)
    }

    fn finish(&mut self, outcome: Outcome) -> bool {
        if self.sync.finish_quiet(&outcome) {
            return false;
        }
        self.sync.finish(&mut self.store, outcome, self.now)
    }

    /// Sends and finishes jobs against `core` until there is none to send.
    fn settle(&mut self, core: &mut FakeCore) {
        for _ in 0..4 {
            let Some(job) = self.send() else {
                return;
            };
            let outcome = core.run(job);
            self.finish(outcome);
        }
    }

    fn keep(&mut self, label: &str, cwd: &str) -> String {
        self.store
            .keep_workspace(&scope(), label, cwd, false)
            .unwrap()
    }

    fn open(&mut self, workspace: &str, terminal: &str) -> TabId {
        let location = Location::core_terminal(Panes::single(id(terminal)), false);
        self.store
            .open(scope(), workspace, Some(location), None)
            .unwrap()
    }

    fn labels(&self) -> Vec<String> {
        self.store
            .kept_workspaces(&scope())
            .map(|workspace| workspace.label.clone())
            .collect()
    }

    /// Each shared tab's terminals, by workspace label.
    fn tabs(&self) -> Vec<(String, Vec<String>)> {
        let mut tabs: Vec<_> = self
            .store
            .in_scope(&scope())
            .filter_map(|tab| {
                let Some(Location::CoreTerminal { panes, .. }) = &tab.location else {
                    return None;
                };
                let label = self
                    .store
                    .kept_workspace(&scope(), &tab.workspace_id)?
                    .label
                    .clone();
                let terminals = panes
                    .terminals()
                    .iter()
                    .map(|t| t.as_str().to_owned())
                    .collect();
                Some((label, terminals))
            })
            .collect();
        tabs.sort();
        tabs
    }
}

use crate::browser::TabId;

fn scope() -> Scope {
    Scope::endpoint("ssh:box")
}

fn id(terminal: &str) -> TerminalId {
    TerminalId::new(terminal).unwrap()
}

fn tab(label: &str, terminals: &[&str]) -> (String, Vec<String>) {
    (
        label.to_owned(),
        terminals.iter().map(|t| (*t).to_owned()).collect(),
    )
}
