//! Keeps the index current as agents write: the OS reports changed session
//! files and only those are read again, so a new message is indexed within
//! [`SETTLE`] of landing on disk. A full rescan every [`RESCAN`] catches
//! anything a watcher missed (an overflowed queue, a root created later);
//! with nothing new it only compares file sizes.
use crate::{
    Host, Report, Result, Root, Store, index, index_paths, redact::Redactor, scan::session_file,
};
use notify::{RecursiveMode, Watcher};
use std::{
    collections::BTreeSet,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// How long a burst of writes may go quiet before it is indexed.
pub const SETTLE: Duration = Duration::from_millis(50);
/// The longest a burst is held before it is indexed anyway.
const BURST: Duration = Duration::from_secs(1);
/// How often everything is rescanned, and `changed` hears a heartbeat.
pub const RESCAN: Duration = Duration::from_secs(30);

/// What the watcher has to say after each pass.
#[derive(Debug)]
pub enum Tick<'a> {
    /// Sessions changed; the report names them.
    Changed(&'a Report),
    /// A quiet rescan found nothing new.
    Idle,
}

/// Indexes everything, then follows changes until `stop` is set or
/// `changed` breaks (for example, because its reader went away). `now`
/// reads the current Unix milliseconds.
pub fn watch(
    store: &mut Store,
    host: &Host,
    roots: &[Root],
    redactor: &Redactor,
    stop: &AtomicBool,
    now: impl Fn() -> i64,
    changed: impl FnMut(&Store, Tick<'_>) -> ControlFlow<()>,
) -> Result<()> {
    let (sender, events) = mpsc::channel();
    let follower = Follower {
        watcher: notify::recommended_watcher(sender)?,
        watched: vec![false; roots.len()],
        parents: BTreeSet::new(),
        aliases: Vec::new(),
    };
    watch_loop(
        Scan {
            store,
            host,
            roots,
            redactor,
            stop,
        },
        follower,
        events,
        now,
        changed,
        Instant::now,
    )
}

struct Scan<'a> {
    store: &'a mut Store,
    host: &'a Host,
    roots: &'a [Root],
    redactor: &'a Redactor,
    stop: &'a AtomicBool,
}

fn watch_loop(
    scan: Scan<'_>,
    mut follower: Follower,
    events: mpsc::Receiver<notify::Result<notify::Event>>,
    now: impl Fn() -> i64,
    mut changed: impl FnMut(&Store, Tick<'_>) -> ControlFlow<()>,
    clock: impl Fn() -> Instant,
) -> Result<()> {
    let Scan {
        store,
        host,
        roots,
        redactor,
        stop,
    } = scan;
    follower.follow(roots);
    let report = index(store, host, roots, redactor, now())?;
    if tick(store, &report, &mut changed).is_break() {
        return Ok(());
    }
    let mut rescan_at = clock() + RESCAN;
    while !stop.load(Ordering::Relaxed) {
        let wait = rescan_at.saturating_duration_since(clock());
        match events.recv_timeout(wait.min(Duration::from_secs(1))) {
            Ok(event) => {
                let mut paths = BTreeSet::new();
                let mut seen = collect(event, roots, &follower.aliases, &mut paths);
                let burst = (clock() + BURST).min(rescan_at);
                while clock() < burst && !stop.load(Ordering::Relaxed) {
                    match events.recv_timeout(SETTLE) {
                        Ok(event) => {
                            seen = seen.max(collect(event, roots, &follower.aliases, &mut paths))
                        }
                        Err(_) => break,
                    }
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                // A root that just appeared may already hold files written
                // before its watch began: read everything once.
                // A perpetually ready queue must not starve catch-up scans.
                let rescan = clock() >= rescan_at
                    || seen == Seen::Rescan
                    || (seen == Seen::Parent && follower.follow(roots));
                let report = if rescan {
                    follower.follow(roots);
                    let report = index(store, host, roots, redactor, now())?;
                    rescan_at = clock() + RESCAN;
                    report
                } else {
                    let paths: Vec<PathBuf> = paths.into_iter().collect();
                    index_paths(store, host, roots, &paths, redactor)?
                };
                let flow = if rescan && report.sessions.is_empty() {
                    changed(store, Tick::Idle)
                } else {
                    tick(store, &report, &mut changed)
                };
                if flow.is_break() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) if clock() >= rescan_at => {
                follower.follow(roots);
                let report = index(store, host, roots, redactor, now())?;
                let flow = if report.sessions.is_empty() {
                    changed(store, Tick::Idle)
                } else {
                    tick(store, &report, &mut changed)
                };
                if flow.is_break() {
                    break;
                }
                rescan_at = clock() + RESCAN;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// The OS watches: each root that exists, recursively, and for each root
/// not created yet, its nearest existing parent alone, so the moment an
/// agent first creates its session directory is noticed.
struct Follower {
    watcher: notify::RecommendedWatcher,
    watched: Vec<bool>,
    parents: BTreeSet<PathBuf>,
    /// FSEvents reports canonical paths, even when a configured root uses
    /// an OS alias such as /var -> /private/var. Keep cursor paths unchanged.
    aliases: Vec<Alias>,
}

struct Alias {
    watched: PathBuf,
    configured: PathBuf,
}

fn remember_alias(aliases: &mut Vec<Alias>, path: &Path) {
    if let Ok(watched) = path.canonicalize()
        && watched != path
        && !aliases.iter().any(|alias| alias.configured == path)
    {
        aliases.push(Alias {
            watched,
            configured: path.to_owned(),
        });
    }
}

fn configured_path(path: PathBuf, aliases: &[Alias]) -> PathBuf {
    let Some((alias, suffix)) = aliases
        .iter()
        .filter_map(|alias| {
            path.strip_prefix(&alias.watched)
                .ok()
                .map(|suffix| (alias, suffix))
        })
        .max_by_key(|(alias, _)| alias.watched.components().count())
    else {
        return path;
    };
    alias.configured.join(suffix)
}

impl Follower {
    /// Watches what can be watched now; `true` when a root newly is.
    fn follow(&mut self, roots: &[Root]) -> bool {
        let mut grew = false;
        for (root, watched) in roots.iter().zip(self.watched.iter_mut()) {
            if *watched {
                continue;
            }
            if root.dir.is_dir() {
                *watched = self
                    .watcher
                    .watch(&root.dir, RecursiveMode::Recursive)
                    .is_ok();
                grew |= *watched;
                if *watched {
                    remember_alias(&mut self.aliases, &root.dir);
                }
                continue;
            }
            let Some(parent) = root.dir.ancestors().skip(1).find(|dir| dir.is_dir()) else {
                continue;
            };
            if !self.parents.contains(parent)
                && self
                    .watcher
                    .watch(parent, RecursiveMode::NonRecursive)
                    .is_ok()
            {
                self.parents.insert(parent.to_owned());
                remember_alias(&mut self.aliases, parent);
            }
        }
        grew
    }
}

/// What a batch of watcher events asks for, least to most work.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Seen {
    /// Only the session files collected.
    Files,
    /// Something appeared on the way to a root not created yet.
    Parent,
    /// The watcher lost track: read everything.
    Rescan,
}

fn tick(
    store: &Store,
    report: &Report,
    changed: &mut impl FnMut(&Store, Tick<'_>) -> ControlFlow<()>,
) -> ControlFlow<()> {
    if report.sessions.is_empty() {
        return ControlFlow::Continue(());
    }
    changed(store, Tick::Changed(report))
}

/// Adds the session files `event` names to `paths`, and says whether it
/// asks for more than reading them.
fn collect(
    event: notify::Result<notify::Event>,
    roots: &[Root],
    aliases: &[Alias],
    paths: &mut BTreeSet<PathBuf>,
) -> Seen {
    let Ok(event) = event else {
        return Seen::Rescan;
    };
    if event.need_rescan() {
        return Seen::Rescan;
    }
    // Opening a file is not a change, and the indexer opens every file it
    // reads: following opens would read each file again forever.
    if let notify::EventKind::Access(access) = event.kind
        && access != notify::event::AccessKind::Close(notify::event::AccessMode::Write)
    {
        return Seen::Files;
    }
    let mut seen = Seen::Files;
    for path in event.paths {
        // Translate only the registered prefix. Canonicalizing each event
        // would follow links inside the root and bypass the no-follow opener.
        let path = configured_path(path, aliases);
        if roots.iter().any(|root| session_file(&root.dir, &path)) {
            paths.insert(path);
        } else if roots.iter().any(|root| root.dir.starts_with(&path)) {
            seen = Seen::Parent;
        }
    }
    seen
}

#[cfg(test)]
mod tests;
