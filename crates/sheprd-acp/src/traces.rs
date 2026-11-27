//! `sheprd-acp traces`: this host's index of agent sessions. `index` scans
//! the agents' session files into it; `watch` keeps it current as agents
//! write and prints each changed session; `list` and `activity` read it.
//! Sheprd runs `exchange` on connected hosts, so parsing and redaction happen
//! where the files are and every host retains local and forwarded records.
//!
//! `watch --json` is a stream of JSON lines: one `host` line first, then a
//! `session` line for each session that changed, and a `heartbeat` line
//! after each quiet rescan, so a reader can tell a live host from a silent
//! one. It stops when its reader goes away.

use anyhow::{Context, Result};
use serde::Serialize;
use sheprd_client::herdr::AgentKind;
use sheprd_traces::{Host, Session, Store, Tick, redact::Redactor};
use std::{
    io::{self, Write},
    ops::ControlFlow,
    sync::atomic::AtomicBool,
};

/// The sessions `list` shows unless told otherwise.
pub(crate) const DEFAULT_LIMIT: usize = 50;
/// The days `activity` covers unless told otherwise.
pub(crate) const DEFAULT_DAYS: u32 = 30;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// One bounded, bidirectional replication exchange over stdin/stdout.
    Exchange,
    Peers {
        json: bool,
    },
    Index {
        json: bool,
    },
    Watch {
        json: bool,
    },
    List {
        agent: Option<AgentKind>,
        limit: usize,
        json: bool,
    },
    Overlap {
        days: u32,
        agent: Option<AgentKind>,
        host: Option<String>,
        at: Option<i64>,
        json: bool,
    },
    Activity {
        days: u32,
        json: bool,
    },
}

pub(crate) fn run(command: Command) -> Result<()> {
    let path = sheprd_traces::path().context("no state directory for traces")?;
    let mut store = Store::open(&path)?;
    let now = chrono::Utc::now().timestamp_millis();
    let local = || -> Result<_> {
        let home = std::env::home_dir().context("no home directory")?;
        let state = path.parent().context("traces index has no directory")?;
        let host = Host::local(state)?;
        let cwd = std::env::current_dir().context("no working directory for traces")?;
        let roots = sheprd_traces::configured_roots(&home, &cwd, |name| std::env::var_os(name));
        Ok((host, roots, Redactor::new(Some(&home))))
    };
    match command {
        Command::Exchange => {
            let (host, roots, redactor) = local()?;
            sheprd_traces::index(&mut store, &host, &roots, &redactor, now)?;
            sheprd_traces::sync::serve(&mut store, io::stdin().lock(), io::stdout().lock())?;
        }
        Command::Peers { json } => {
            let peers = store.sync_peers()?;
            if json {
                println!("{}", serde_json::to_string(&peers)?);
            } else {
                for peer in peers {
                    println!(
                        "{}  received={} sent={}  {}",
                        peer.route,
                        peer.received,
                        peer.sent,
                        peer.synced_at
                            .map_or_else(|| "not synchronized yet".into(), time)
                    );
                }
            }
        }
        Command::Watch { json } => {
            let (host, roots, redactor) = local()?;
            let stop = AtomicBool::new(false);
            let mut out = io::stdout().lock();
            if json {
                line(
                    &mut out,
                    &Line::Host {
                        v: WIRE,
                        host: &host,
                    },
                )?;
            }
            sheprd_traces::watch(
                &mut store,
                &host,
                &roots,
                &redactor,
                &stop,
                || chrono::Utc::now().timestamp_millis(),
                |store, tick| match emit(&mut out, store, tick, json) {
                    Ok(()) => ControlFlow::Continue(()),
                    // The reader went away (or the index failed): stop.
                    Err(_) => ControlFlow::Break(()),
                },
            )?;
        }
        Command::Index { json } => {
            let (host, roots, redactor) = local()?;
            let report = sheprd_traces::index(&mut store, &host, &roots, &redactor, now)?;
            if json {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                println!(
                    "{} files, {} changed, {} new events",
                    report.files, report.changed, report.events
                );
                for skipped in &report.skipped {
                    eprintln!("skipped {skipped}");
                }
            }
        }
        Command::List { agent, limit, json } => {
            let sessions = store.sessions(agent, limit)?;
            if json {
                println!("{}", serde_json::to_string(&sessions)?);
                return Ok(());
            }
            for session in sessions {
                println!("{}", summary(&session));
            }
        }
        Command::Overlap {
            days,
            agent,
            host,
            at,
            json,
        } => {
            let host = host.as_deref().map(|id| [id]);
            let hosts = host.as_ref().map(|ids| ids.as_slice());
            if let Some(at) = at {
                let sessions = store.active_sessions(at, hosts, agent)?;
                if json {
                    println!("{}", serde_json::to_string(&sessions)?);
                } else {
                    for session in sessions {
                        println!("{}", summary(&session));
                    }
                }
            } else {
                let overlap = store.overlap(now, days, hosts, agent)?;
                if json {
                    println!("{}", serde_json::to_string(&overlap)?);
                } else {
                    println!("{} active in the last 15 minutes", overlap.recent.count);
                    for peak in overlap.daily {
                        println!("{}  {} peak", time(peak.at), peak.count);
                    }
                }
            }
        }
        Command::Activity { days, json } => {
            let activity = store.activity(now - i64::from(days) * DAY_MS)?;
            if json {
                println!("{}", serde_json::to_string(&activity)?);
                return Ok(());
            }
            for day in activity {
                println!(
                    "{}  {:<8} {:>4} sessions {:>6} msgs",
                    day.day,
                    day.agent.name(),
                    day.sessions,
                    day.messages
                );
            }
        }
    }
    Ok(())
}

/// The version of `watch --json`'s lines; a reader refuses newer ones.
const WIRE: u32 = 1;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Line<'a> {
    Host { v: u32, host: &'a Host },
    Session { session: &'a Session },
    Heartbeat { at: i64 },
}

fn line(out: &mut impl Write, line: &Line<'_>) -> Result<()> {
    serde_json::to_writer(&mut *out, line)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

fn emit(out: &mut impl Write, store: &Store, tick: Tick<'_>, json: bool) -> Result<()> {
    let Tick::Changed(report) = tick else {
        let at = chrono::Utc::now().timestamp_millis();
        return if json {
            line(out, &Line::Heartbeat { at })
        } else {
            Ok(())
        };
    };
    for id in &report.sessions {
        let Some(session) = store.session(id)? else {
            continue;
        };
        if json {
            line(out, &Line::Session { session: &session })?;
        } else {
            writeln!(out, "{}", summary(&session))?;
            out.flush()?;
        }
    }
    Ok(())
}

fn summary(session: &Session) -> String {
    format!(
        "{}  {:<8} {:<12} {:>5} msgs  {:<22} {}",
        session.updated_at.map_or_else(|| "-".into(), time),
        session.agent.name(),
        session.host,
        session.messages,
        session.model.as_deref().unwrap_or("-"),
        session.title.as_deref().unwrap_or("(untitled)"),
    )
}

fn time(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
