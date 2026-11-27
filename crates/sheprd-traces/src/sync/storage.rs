//! Durable journal, migration, and idempotent import, all under SQLite transactions.
use super::{BATCH_BYTES, Batch, Error, MAX_EVENTS, MAX_RECORDS, Record, Stamp};
use crate::{
    Event, Host, Result,
    store::{events_on, recount, session_on},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

const SCHEMA: &str = "
CREATE TABLE trace_replica (id INTEGER PRIMARY KEY CHECK(id = 1), replica TEXT NOT NULL, clock INTEGER NOT NULL);
CREATE TABLE trace_heads (origin TEXT PRIMARY KEY, sequence INTEGER NOT NULL);
CREATE TABLE trace_journal (
    position INTEGER PRIMARY KEY AUTOINCREMENT, origin TEXT NOT NULL,
    sequence INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(origin, sequence)
);
CREATE TABLE trace_versions (
    session_id TEXT PRIMARY KEY, gen_clock INTEGER NOT NULL, gen_replica TEXT NOT NULL,
    clock INTEGER NOT NULL, replica TEXT NOT NULL
);
CREATE TABLE trace_peers (
    route TEXT PRIMARY KEY, replica TEXT NOT NULL, received INTEGER NOT NULL,
    sent INTEGER NOT NULL, synced_at INTEGER
);
";

pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(SCHEMA)?;
    tx.execute(
        "INSERT INTO trace_replica VALUES (1, ?1, 0)",
        [uuid::Uuid::new_v4().to_string()],
    )?;
    // Seed schema-1 history in bounded pages, atomically with the migration.
    let mut after = String::new();
    loop {
        let id: Option<String> = tx
            .query_row(
                "SELECT session_id FROM sessions WHERE session_id > ?1 ORDER BY session_id LIMIT 1",
                [&after],
                |row| row.get(0),
            )
            .optional()?;
        let Some(id) = id else { break };
        after = id.clone();
        let Some(session) = session_on(tx, &id)? else {
            continue;
        };
        let os = tx.query_row(
            "SELECT os FROM hosts WHERE host_id = ?1",
            [&session.host_id],
            |row| row.get(0),
        )?;
        let host = Host {
            id: session.host_id,
            label: session.host,
            user: session.user,
            os,
        };
        let mut from = 0;
        loop {
            let events = events_on(tx, &id, from, MAX_EVENTS)?;
            record_local(tx, &host, &id, &events, from == 0)?;
            let Some(last) = events.last() else { break };
            from = last.seq.checked_add(1).ok_or(Error::Limit)?;
            if events.len() < MAX_EVENTS {
                break;
            }
        }
    }
    Ok(())
}

pub(crate) fn identity(connection: &Connection) -> Result<String> {
    Ok(connection.query_row(
        "SELECT replica FROM trace_replica WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
}

fn versions(connection: &Connection, id: &str) -> Result<Option<(Stamp, Stamp)>> {
    Ok(connection.query_row(
        "SELECT gen_clock, gen_replica, clock, replica FROM trace_versions WHERE session_id = ?1",
        [id], |row| Ok((
            Stamp { clock: row.get(0)?, replica: row.get(1)? },
            Stamp { clock: row.get(2)?, replica: row.get(3)? },
        )),
    ).optional()?)
}

fn set_versions(tx: &Transaction<'_>, id: &str, generation: &Stamp, version: &Stamp) -> Result<()> {
    tx.execute(
        "INSERT INTO trace_versions VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id) DO UPDATE SET gen_clock=?2, gen_replica=?3, clock=?4, replica=?5",
        params![
            id,
            generation.clock,
            generation.replica,
            version.clock,
            version.replica
        ],
    )?;
    Ok(())
}

pub(crate) fn record_local(
    tx: &Transaction<'_>,
    host: &Host,
    id: &str,
    events: &[Event],
    reset: bool,
) -> Result<()> {
    let replica = identity(tx)?;
    let clock: i64 = tx.query_row("SELECT clock FROM trace_replica WHERE id = 1", [], |row| {
        row.get(0)
    })?;
    let clock = clock.checked_add(1).ok_or(Error::Limit)?;
    tx.execute("UPDATE trace_replica SET clock=?1 WHERE id=1", [clock])?;
    let version = Stamp {
        clock,
        replica: replica.clone(),
    };
    let generation = match versions(tx, id)? {
        Some((generation, _)) if !reset => generation,
        _ => version.clone(),
    };
    let Some(session) = session_on(tx, id)? else {
        return Err(Error::Invalid("missing local session").into());
    };
    set_versions(tx, id, &generation, &version)?;
    // One metadata-only record for updates without events.
    for chunk in events
        .chunks(MAX_EVENTS)
        .chain(events.is_empty().then_some(&[][..]))
    {
        let sequence = head(tx, &replica)?.checked_add(1).ok_or(Error::Limit)?;
        let record = Record {
            origin: replica.clone(),
            sequence,
            version: version.clone(),
            generation: generation.clone(),
            host: host.clone(),
            session: session.clone(),
            events: chunk.to_vec(),
        };
        append(tx, &record)?;
    }
    Ok(())
}

fn head(connection: &Connection, origin: &str) -> Result<i64> {
    Ok(connection
        .query_row(
            "SELECT sequence FROM trace_heads WHERE origin=?1",
            [origin],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

fn append(tx: &Transaction<'_>, record: &Record) -> Result<()> {
    let payload = serde_json::to_string(record).map_err(Error::Json)?;
    if payload.len() > BATCH_BYTES {
        return Err(Error::Limit.into());
    }
    tx.execute(
        "INSERT INTO trace_journal(origin, sequence, payload) VALUES (?1, ?2, ?3)",
        params![record.origin, record.sequence, payload],
    )?;
    tx.execute(
        "INSERT INTO trace_heads VALUES (?1, ?2) ON CONFLICT(origin) DO UPDATE SET sequence=?2",
        params![record.origin, record.sequence],
    )?;
    Ok(())
}

pub(crate) fn journal_end(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT coalesce(max(position), 0) FROM trace_journal",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn export(connection: &Connection, after: i64) -> Result<Batch> {
    let end = journal_end(connection)?;
    if after < 0 || after > end {
        return Err(Error::Invalid("journal cursor").into());
    }
    let mut query = connection.prepare_cached("SELECT position, payload FROM trace_journal WHERE position > ?1 ORDER BY position LIMIT ?2")?;
    let mut rows = query.query(params![after, MAX_RECORDS as i64])?;
    let mut batch = Batch {
        start: after,
        through: after,
        more: false,
        records: Vec::new(),
    };
    let mut bytes = 0;
    while let Some(row) = rows.next()? {
        let payload: String = row.get(1)?;
        if bytes + payload.len() > BATCH_BYTES {
            break;
        }
        bytes += payload.len();
        batch
            .records
            .push(serde_json::from_str(&payload).map_err(Error::Json)?);
        batch.through = row.get(0)?;
    }
    batch.more = batch.through < end;
    Ok(batch)
}

pub(crate) fn import(tx: &Transaction<'_>, batch: &Batch) -> Result<()> {
    super::protocol::validate(batch)?;
    let local = identity(tx)?;
    let mut rebase = std::collections::BTreeSet::new();
    for record in &batch.records {
        let sequence = head(tx, &record.origin)?;
        if record.sequence <= sequence {
            let payload: String = tx.query_row(
                "SELECT payload FROM trace_journal WHERE origin=?1 AND sequence=?2",
                params![record.origin, record.sequence],
                |row| row.get(0),
            )?;
            let accepted: Record = serde_json::from_str(&payload).map_err(Error::Json)?;
            if accepted != *record {
                return Err(Error::Conflict.into());
            }
            continue;
        }
        if record.origin == local {
            return Err(Error::Conflict.into());
        }
        if record.sequence != sequence.checked_add(1).ok_or(Error::Limit)? {
            return Err(Error::Gap.into());
        }
        let clock: i64 = tx.query_row("SELECT clock FROM trace_replica WHERE id=1", [], |row| {
            row.get(0)
        })?;
        // Forwarded journals retain causal order. A new clock can only be the
        // successor of a clock already received, not an arbitrary peer value
        // that could exhaust the local counter and prevent future writes.
        if record.version.clock > clock.saturating_add(1) {
            return Err(Error::Gap.into());
        }
        let owns: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM file_cursors WHERE session_id=?1)",
            [&record.session.id],
            |row| row.get(0),
        )?;
        if owns {
            // After an index rebuild the local files remain authoritative.
            // Learn the old clock, then republish their current contents above
            // it rather than letting a stale replica restore a truncated file.
            if versions(tx, &record.session.id)?.is_some_and(|(generation, version)| {
                record.generation > generation
                    || (record.generation == generation && record.version > version)
            }) {
                rebase.insert(record.session.id.clone());
            }
        } else {
            apply(tx, record)?;
        }
        tx.execute(
            "UPDATE trace_replica SET clock=max(clock, ?1) WHERE id=1",
            [record.version.clock],
        )?;
        append(tx, record)?;
    }
    for id in rebase {
        let Some(session) = session_on(tx, &id)? else {
            continue;
        };
        let os = tx.query_row(
            "SELECT os FROM hosts WHERE host_id=?1",
            [&session.host_id],
            |row| row.get(0),
        )?;
        let host = Host {
            id: session.host_id,
            label: session.host,
            user: session.user,
            os,
        };
        let mut from = 0;
        loop {
            let events = events_on(tx, &id, from, MAX_EVENTS)?;
            record_local(tx, &host, &id, &events, from == 0)?;
            let Some(last) = events.last() else { break };
            from = last.seq.checked_add(1).ok_or(Error::Limit)?;
            if events.len() < MAX_EVENTS {
                break;
            }
        }
    }
    Ok(())
}

fn apply(tx: &Transaction<'_>, record: &Record) -> Result<()> {
    let s = &record.session;
    let known = versions(tx, &s.id)?;
    if known
        .as_ref()
        .is_some_and(|(generation, _)| record.generation < *generation)
    {
        return Ok(());
    }
    if let Some(existing) = session_on(tx, &s.id)?
        && (existing.host_id != s.host_id
            || existing.agent != s.agent
            || super::account(&existing.user) != super::account(&s.user))
    {
        return Err(Error::Conflict.into());
    }
    let newer_generation = known
        .as_ref()
        .is_none_or(|(generation, _)| record.generation > *generation);
    if newer_generation {
        tx.execute("DELETE FROM events WHERE session_id=?1", [&s.id])?;
    }
    if newer_generation
        || known
            .as_ref()
            .is_none_or(|(_, version)| record.version >= *version)
    {
        tx.execute(
            "INSERT INTO hosts(host_id,label,os,kind) VALUES (?1,?2,?3,'peer')
             ON CONFLICT(host_id) DO UPDATE SET label=?2,os=?3 WHERE kind != 'local'",
            params![record.host.id, record.host.label, record.host.os],
        )?;
        tx.execute(
            "INSERT INTO sessions(session_id,host_id,agent,native_id,user,title,model,cwd,branch,agent_version,started_at,updated_at,tokens_in,tokens_out,visibility)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(session_id) DO UPDATE SET native_id=?4,title=?6,model=?7,cwd=?8,branch=?9,agent_version=?10,started_at=?11,updated_at=?12,tokens_in=?13,tokens_out=?14,visibility=?15",
            params![s.id,s.host_id,s.agent.name(),s.native_id,s.user,s.title,s.model,s.cwd,s.branch,s.agent_version,s.started_at,s.updated_at,s.tokens_in,s.tokens_out,s.visibility.as_str()],
        )?;
        set_versions(tx, &s.id, &record.generation, &record.version)?;
    }
    for event in &record.events {
        tx.execute("INSERT OR IGNORE INTO events(session_id,seq,at,kind,tool,text) VALUES (?1,?2,?3,?4,?5,?6)", params![s.id,event.seq,event.at,event.kind.as_str(),event.tool,event.text])?;
    }
    recount(tx, &s.id)
}
