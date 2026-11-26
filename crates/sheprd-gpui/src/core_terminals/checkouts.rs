//! The Git checkout of each workspace the app keeps for Herdr 2: its branch,
//! its repository, and whether it is a linked worktree, which Herdr 1
//! reports for its own workspaces and herdr-core does not. Asked on a worker
//! thread, one batch at a time, and again every so often, as a branch
//! changes under a running shell.

mod lookup;

use lookup::Lookup;
use sheprd_vcs::git::Identity;
use std::{
    collections::HashMap,
    sync::mpsc,
    time::{Duration, Instant},
};

/// How often the folders are asked again.
const REFRESH: Duration = Duration::from_secs(10);
/// How long one folder's answer may take.
const DEADLINE: Duration = Duration::from_secs(5);

struct Answer {
    cwd: String,
    identity: Lookup,
    upstream: Option<(usize, usize)>,
}

struct Answers {
    folders: Vec<Answer>,
    home: Option<String>,
}

/// What the folders last answered, and the batch being asked.
#[derive(Default)]
pub(super) struct Checkouts {
    known: HashMap<String, Option<Identity>>,
    home: Option<String>,
    upstream: HashMap<String, (usize, usize)>,
    asking: Option<mpsc::Receiver<Answers>>,
    asked: Option<Instant>,
}

impl Checkouts {
    /// What `cwd` last answered: `None` while it has not, or when it is no
    /// Git checkout.
    pub(super) fn get(&self, cwd: &str) -> Option<&Identity> {
        self.known.get(cwd)?.as_ref()
    }

    /// Whether Git answered for `cwd`, a checkout or none: until then a
    /// name read from the folder alone may not be the one it ends with.
    pub(super) fn answered(&self, cwd: &str) -> bool {
        self.known.contains_key(cwd)
    }

    pub(super) fn home(&self) -> Option<&str> {
        self.home.as_deref()
    }

    pub(super) fn upstream(&self, cwd: &str) -> Option<(usize, usize)> {
        self.upstream.get(cwd).copied()
    }

    /// Takes a finished batch and starts the next one when `folders` holds
    /// one never asked or the last answers are old. Whether answers changed.
    pub(super) fn poll(&mut self, folders: Vec<String>, now: Instant) -> bool {
        self.poll_on(folders, now, sheprd_client::ConnectTarget::Local)
    }

    pub(super) fn poll_on(
        &mut self,
        folders: Vec<String>,
        now: Instant,
        target: sheprd_client::ConnectTarget,
    ) -> bool {
        let mut changed = false;
        if let Some(asking) = &self.asking {
            match asking.try_recv() {
                Ok(answers) => {
                    if let Some(home) = answers.home {
                        changed |= self.home.as_ref() != Some(&home);
                        self.home = Some(home);
                    }
                    for Answer {
                        cwd,
                        identity,
                        upstream,
                    } in answers.folders
                    {
                        let identity = match identity {
                            Lookup::Ready(identity) => identity,
                            Lookup::Unavailable => {
                                // Remember the attempt to retain the normal retry interval,
                                // but never replace a previous successful answer.
                                self.known.entry(cwd).or_insert(None);
                                continue;
                            }
                        };
                        changed |= self.upstream.get(&cwd).copied() != upstream;
                        if let Some(counts) = upstream {
                            self.upstream.insert(cwd.clone(), counts);
                        } else {
                            self.upstream.remove(&cwd);
                        }
                        changed |= self.known.get(&cwd) != Some(&identity);
                        self.known.insert(cwd, identity);
                    }
                    self.asking = None;
                }
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => self.asking = None,
            }
        }
        let before = self.known.len();
        self.known.retain(|cwd, _| folders.contains(cwd));
        self.upstream.retain(|cwd, _| folders.contains(cwd));
        changed |= self.known.len() != before;
        let unasked = folders.iter().any(|cwd| !self.known.contains_key(cwd));
        let stale = self.asked.is_none_or(|asked| now >= asked + REFRESH);
        if folders.is_empty() || !(unasked || stale) {
            return changed;
        }
        let home = self.home.clone();
        let (send, receive) = mpsc::sync_channel(1);
        let started = std::thread::Builder::new()
            .name("herdr-core-checkouts".into())
            .spawn(move || {
                let home = home.or_else(|| home_on(&target));
                let folders = folders
                    .into_iter()
                    .map(|cwd| {
                        let deadline = Instant::now() + DEADLINE;
                        let identity = lookup::read(&target, &cwd, deadline);
                        let upstream =
                            if matches!(&identity, Lookup::Ready(Some(_))) && !target.is_remote() {
                                sheprd_vcs::git::upstream::read(&cwd, deadline, &|| false)
                                    .ok()
                                    .flatten()
                            } else {
                                None
                            };
                        Answer {
                            cwd,
                            identity,
                            upstream,
                        }
                    })
                    .collect();
                let _ = send.send(Answers { folders, home });
            });
        match started {
            Ok(_) => self.asking = Some(receive),
            Err(error) => tracing::warn!(%error, "Could not start a checkout worker"),
        }
        self.asked = Some(now);
        changed
    }

    #[cfg(test)]
    pub(crate) fn seed(&mut self, cwd: &str, identity: Option<Identity>) {
        // An older real worker must not overwrite the fixture's answer.
        self.asking = None;
        self.known.insert(cwd.to_owned(), identity);
        self.asked = Some(Instant::now());
    }
}

/// Runs only on the checkout worker. A remote home is read on that host,
/// never inferred from a path or borrowed from the desktop environment.
fn home_on(target: &sheprd_client::ConnectTarget) -> Option<String> {
    if !target.is_remote() {
        return sheprd_client::local_path::home_dir()
            .map(|home| home.to_string_lossy().into_owned());
    }
    let host = sheprd_remote::teleport::Host::new(target).ok()?;
    let output = host
        .exec(
            "sh",
            &["-c", "printf '%s' \"$HOME\""],
            DEADLINE,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .ok()?;
    String::from_utf8(output)
        .ok()
        .filter(|home| home.starts_with('/') && !home.chars().any(char::is_control))
}

#[cfg(test)]
impl super::CoreTerminals {
    pub(crate) fn seed_remote_checkout_for_test(
        &mut self,
        scope: &crate::browser::Scope,
        cwd: &str,
        identity: Option<Identity>,
    ) {
        if let Some(remote) = self.remote.get_mut(scope) {
            remote.checkouts.seed(cwd, identity);
        }
    }

    /// Answers for `cwd` as Git would, for tests outside this module.
    pub(crate) fn seed_checkout(&mut self, cwd: &str, identity: Option<Identity>) {
        self.checkouts.seed(cwd, identity);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
