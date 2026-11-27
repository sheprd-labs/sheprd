//! The index, open: the schema, writes from a scan, and the reads the CLI
//! and the app need (sessions, events, activity).
mod overlap;
pub use overlap::{Overlap, OverlapCache, OverlapPoint};

use crate::{
    Error, Host, Result,
    model::{Activity, Event, EventKind, Session, Visibility},
    parse::{Meta, Parser},
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use sheprd_client::herdr::AgentKind;
use std::{path::Path, time::Duration};

/// The newest schema this build reads and writes.
pub const SCHEMA_VERSION: i64 = 3;
/// The most sessions or events one read returns.
pub const MAX_LIMIT: usize = 1000;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// `hosts.kind` is `local` for the machine that wrote the row, `peer` for
/// another Sheprd's, and `ssh` for a machine read over SSH. Events keep
/// their rowid so a peer can pull rows added since a cursor; `events_fts`
/// is an external-content index the triggers keep in step.
const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS hosts (
    host_id TEXT PRIMARY KEY NOT NULL,
    label TEXT NOT NULL,
    os TEXT NOT NULL,
    kind TEXT NOT NULL,
    last_seen_at INTEGER
);
CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    host_id TEXT NOT NULL REFERENCES hosts(host_id),
    agent TEXT NOT NULL,
    native_id TEXT NOT NULL,
    user TEXT NOT NULL,
    title TEXT,
    model TEXT,
    cwd TEXT,
    branch TEXT,
    agent_version TEXT,
    started_at INTEGER,
    updated_at INTEGER,
    messages INTEGER NOT NULL DEFAULT 0,
    user_messages INTEGER NOT NULL DEFAULT 0,
    tool_calls INTEGER NOT NULL DEFAULT 0,
    errors INTEGER NOT NULL DEFAULT 0,
    tokens_in INTEGER NOT NULL DEFAULT 0,
    tokens_out INTEGER NOT NULL DEFAULT 0,
    visibility TEXT NOT NULL DEFAULT 'private'
);
CREATE INDEX IF NOT EXISTS sessions_updated ON sessions(updated_at);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    at INTEGER,
    kind TEXT NOT NULL,
    tool TEXT,
    text TEXT NOT NULL,
    UNIQUE (session_id, seq)
);
CREATE INDEX IF NOT EXISTS events_at ON events(at);
CREATE VIRTUAL TABLE IF NOT EXISTS events_fts
    USING fts5(tool, text, content = 'events', content_rowid = 'id');
CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
    INSERT INTO events_fts(rowid, tool, text) VALUES (new.id, new.tool, new.text);
END;
CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
    INSERT INTO events_fts(events_fts, rowid, tool, text)
        VALUES ('delete', old.id, old.tool, old.text);
END;
CREATE TABLE IF NOT EXISTS file_cursors (
    host_id TEXT NOT NULL,
    path TEXT NOT NULL,
    session_id TEXT NOT NULL,
    ident TEXT NOT NULL,
    offset INTEGER NOT NULL,
    parser TEXT NOT NULL,
    PRIMARY KEY (host_id, path)
);
";

const COLUMNS: &str = "s.session_id, s.host_id, h.label, s.agent, s.native_id, s.user, s.title, \
     s.model, s.cwd, s.branch, s.agent_version, s.started_at, s.updated_at, s.messages, \
     s.user_messages, s.tool_calls, s.errors, s.tokens_in, s.tokens_out, s.visibility";

/// Where one file's last scan stopped.
#[derive(Clone, Debug)]
pub(crate) struct Cursor {
    pub session_id: String,
    /// The file's identity (device and inode where known); a new one means
    /// the file was replaced and is read again from the start.
    pub ident: String,
    pub offset: u64,
    pub parser: Parser,
}

/// The index, open.
#[derive(Debug)]
pub struct Store {
    pub(crate) connection: Connection,
}

impl Store {
    /// Opens `path`, creating the private directory and file when missing,
    /// and migrates it to [`SCHEMA_VERSION`].
    pub fn open(path: &Path) -> Result<Self> {
        sheprd_client::private_file::prepare(path)?;
        Self::with(Connection::open(path)?)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(mut connection: Connection) -> Result<Self> {
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "wal")?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if found > SCHEMA_VERSION {
            return Err(Error::NewerSchema {
                found,
                known: SCHEMA_VERSION,
            });
        }
        if found < 1 {
            transaction.execute_batch(SCHEMA_V1)?;
        }
        if found < 2 {
            crate::sync::storage::migrate(&transaction)?;
        }
        if found < 3 {
            crate::sync::tags::migrate(&transaction)?;
        }
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
        Ok(Self { connection })
    }

    /// Records `host` as seen at `now` (Unix milliseconds).
    pub(crate) fn host(&self, host: &Host, kind: &str, now: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO hosts (host_id, label, os, kind, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(host_id) DO UPDATE SET label = ?2, os = ?3, kind = ?4, last_seen_at = ?5",
            params![host.id, host.label, host.os, kind, now],
        )?;
        Ok(())
    }

    pub(crate) fn cursor(&self, host_id: &str, path: &str) -> Result<Option<Cursor>> {
        let row = self
            .connection
            .query_row(
                "SELECT session_id, ident, offset, parser FROM file_cursors
                 WHERE host_id = ?1 AND path = ?2",
                params![host_id, path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(session_id, ident, offset, parser)| {
            Ok(Cursor {
                session_id,
                ident,
                offset: u64::try_from(offset).unwrap_or_default(),
                parser: serde_json::from_str(&parser).map_err(Error::State)?,
            })
        })
        .transpose()
    }

    /// Keeps what one scan of one file found, in one transaction: the
    /// session's metadata, its new events, its recounted totals, and the
    /// file's cursor. `restart` first drops the session's events, for a
    /// file that was replaced or shrank.
    pub(crate) fn apply(&mut self, update: &Update<'_>) -> Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        upsert_session(&transaction, update)?;
        if update.restart {
            transaction.execute(
                "DELETE FROM events WHERE session_id = ?1",
                [update.cursor.session_id.as_str()],
            )?;
        }
        let mut added = 0;
        {
            let mut insert = transaction.prepare_cached(
                "INSERT OR IGNORE INTO events (session_id, seq, at, kind, tool, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for event in update.events {
                added += insert.execute(params![
                    update.cursor.session_id,
                    event.seq,
                    event.at,
                    event.kind.as_str(),
                    event.tool,
                    event.text,
                ])?;
            }
        }
        recount(&transaction, &update.cursor.session_id)?;
        crate::sync::storage::record_local(
            &transaction,
            update.host,
            &update.cursor.session_id,
            update.events,
            update.restart,
        )?;
        transaction.execute(
            "INSERT INTO file_cursors (host_id, path, session_id, ident, offset, parser)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(host_id, path) DO UPDATE SET
                session_id = ?3, ident = ?4, offset = ?5, parser = ?6",
            params![
                update.host.id,
                update.path,
                update.cursor.session_id,
                update.cursor.ident,
                i64::try_from(update.cursor.offset).unwrap_or(i64::MAX),
                serde_json::to_string(&update.cursor.parser).map_err(Error::State)?,
            ],
        )?;
        transaction.commit()?;
        Ok(added)
    }

    /// The `limit` most recently updated sessions, of `agent` when given.
    pub fn sessions(&self, agent: Option<AgentKind>, limit: usize) -> Result<Vec<Session>> {
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM sessions s JOIN hosts h USING (host_id)
             WHERE ?1 IS NULL OR s.agent = ?1
             ORDER BY s.updated_at DESC NULLS LAST LIMIT ?2"
        ))?;
        let rows = statement.query_map(
            params![agent.map(AgentKind::name), limit.min(MAX_LIMIT) as i64],
            session,
        )?;
        let mut sessions = Vec::new();
        for row in rows {
            // A row of an agent this build does not know is left out.
            sessions.extend(row?);
        }
        Ok(sessions)
    }

    /// The session `session_id`, if indexed.
    pub fn session(&self, session_id: &str) -> Result<Option<Session>> {
        session_on(&self.connection, session_id)
    }

    /// The events of `session_id` from `seq` on, oldest first.
    pub fn events(&self, session_id: &str, from: i64, limit: usize) -> Result<Vec<Event>> {
        events_on(&self.connection, session_id, from, limit)
    }

    /// Sessions and messages per UTC day and agent since `since` (Unix
    /// milliseconds), oldest day first.
    pub fn activity(&self, since: i64) -> Result<Vec<Activity>> {
        self.activity_for(since, i64::MAX, None)
    }

    /// Daily totals over a bounded interval and optional source-host set.
    /// `None` includes every host; an empty set deliberately includes none.
    pub fn activity_for(
        &self,
        since: i64,
        until: i64,
        hosts: Option<&[&str]>,
    ) -> Result<Vec<Activity>> {
        let hosts = hosts.map(|hosts| serde_json::json!(hosts).to_string());
        let mut statement = self.connection.prepare_cached(
            "SELECT date(e.at / 1000, 'unixepoch') AS day, s.agent,
                    count(DISTINCT e.session_id), count(*)
             FROM events e JOIN sessions s USING (session_id)
             WHERE e.at >= ?1 AND e.at <= ?2 AND e.kind IN ('user', 'agent')
               AND (?3 IS NULL OR s.host_id IN (SELECT value FROM json_each(?3)))
             GROUP BY day, s.agent ORDER BY day, s.agent",
        )?;
        let rows = statement.query_map(params![since, until, hosts], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })?;
        let mut activity = Vec::new();
        for row in rows {
            let (day, agent, sessions, messages) = row?;
            if let Some(agent) = AgentKind::parse(&agent) {
                activity.push(Activity {
                    day,
                    agent,
                    sessions,
                    messages,
                });
            }
        }
        Ok(activity)
    }
}

pub(crate) fn session_on(connection: &Connection, session_id: &str) -> Result<Option<Session>> {
    let mut statement = connection.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM sessions s JOIN hosts h USING (host_id)
             WHERE s.session_id = ?1"
    ))?;
    Ok(statement
        .query_row([session_id], session)
        .optional()?
        .flatten())
}

pub(crate) fn events_on(
    connection: &Connection,
    session_id: &str,
    from: i64,
    limit: usize,
) -> Result<Vec<Event>> {
    let mut statement = connection.prepare_cached(
        "SELECT seq, at, kind, tool, text FROM events
             WHERE session_id = ?1 AND seq >= ?2 ORDER BY seq LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![session_id, from, limit.min(MAX_LIMIT) as i64],
        |row| {
            let kind: String = row.get(2)?;
            let Some(kind) = EventKind::parse(&kind) else {
                return Ok(None);
            };
            Ok(Some(Event {
                seq: row.get(0)?,
                at: row.get(1)?,
                kind,
                tool: row.get(3)?,
                text: row.get(4)?,
            }))
        },
    )?;
    let mut events = Vec::new();
    for row in rows {
        events.extend(row?);
    }
    Ok(events)
}

/// What one scan of one file found.
pub(crate) struct Update<'a> {
    pub host: &'a Host,
    pub agent: AgentKind,
    pub path: &'a str,
    pub cursor: &'a Cursor,
    pub events: &'a [Event],
    pub restart: bool,
}

fn upsert_session(transaction: &Transaction<'_>, update: &Update<'_>) -> Result<()> {
    let Meta {
        native_id,
        title,
        model,
        cwd,
        branch,
        agent_version,
        started_at,
        updated_at,
        tokens_in,
        tokens_out,
    } = &update.cursor.parser.meta;
    let native_id = native_id.as_deref().unwrap_or(update.path);
    transaction.execute(
        "INSERT INTO sessions (session_id, host_id, agent, native_id, user, title, model, cwd,
            branch, agent_version, started_at, updated_at, tokens_in, tokens_out)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(session_id) DO UPDATE SET native_id = ?4, title = ?6, model = ?7,
            cwd = ?8, branch = ?9, agent_version = ?10, started_at = ?11, updated_at = ?12,
            tokens_in = ?13, tokens_out = ?14",
        params![
            update.cursor.session_id,
            update.host.id,
            update.agent.name(),
            native_id,
            update.host.user,
            title,
            model,
            cwd,
            branch,
            agent_version,
            started_at,
            updated_at,
            tokens_in,
            tokens_out,
        ],
    )?;
    Ok(())
}

pub(crate) fn recount(transaction: &Transaction<'_>, session_id: &str) -> Result<()> {
    transaction.execute(
        "UPDATE sessions SET
            messages = (SELECT count(*) FROM events
                WHERE session_id = ?1 AND kind IN ('user', 'agent')),
            user_messages = (SELECT count(*) FROM events WHERE session_id = ?1 AND kind = 'user'),
            tool_calls = (SELECT count(*) FROM events
                WHERE session_id = ?1 AND kind = 'tool_call'),
            errors = (SELECT count(*) FROM events WHERE session_id = ?1 AND kind = 'error')
         WHERE session_id = ?1",
        [session_id],
    )?;
    Ok(())
}

fn session(row: &Row<'_>) -> rusqlite::Result<Option<Session>> {
    let agent: String = row.get(3)?;
    let Some(agent) = AgentKind::parse(&agent) else {
        return Ok(None);
    };
    let visibility: String = row.get(19)?;
    Ok(Some(Session {
        id: row.get(0)?,
        host_id: row.get(1)?,
        host: row.get(2)?,
        agent,
        native_id: row.get(4)?,
        user: row.get(5)?,
        title: row.get(6)?,
        model: row.get(7)?,
        cwd: row.get(8)?,
        branch: row.get(9)?,
        agent_version: row.get(10)?,
        started_at: row.get(11)?,
        updated_at: row.get(12)?,
        messages: row.get(13)?,
        user_messages: row.get(14)?,
        tool_calls: row.get(15)?,
        errors: row.get(16)?,
        tokens_in: row.get(17)?,
        tokens_out: row.get(18)?,
        visibility: Visibility::parse(&visibility).unwrap_or_default(),
    }))
}
