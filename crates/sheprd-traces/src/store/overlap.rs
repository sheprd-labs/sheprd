//! Daily peaks over distinct sessions with recent model activity.
use super::{COLUMNS, MAX_LIMIT, Store, session};
use crate::{ActiveAgent, Result, Session};
use rusqlite::{Connection, params};
use serde::Serialize;
use sheprd_client::herdr::AgentKind;
use std::collections::{BTreeSet, HashMap};

const DAY: i64 = 86_400_000;
const WINDOW: i64 = 15 * 60_000;
const ACTIVITY: &str = "FROM events e JOIN sessions s USING (session_id)
    WHERE e.at > ?1 AND e.at <= ?2 AND e.kind IN ('agent','thinking','tool_call')
    AND (?3 IS NULL OR s.host_id IN (SELECT value FROM json_each(?3)))
    AND (?4 IS NULL OR s.agent = ?4)";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct OverlapPoint {
    /// Unix milliseconds. Ties use the earliest instant in the UTC day.
    pub at: i64,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Overlap {
    pub recent: OverlapPoint,
    pub recent_by_agent: Vec<ActiveAgent>,
    /// One point per UTC day including empty days, oldest first.
    pub daily: Vec<OverlapPoint>,
}

/// Previous daily peaks for one viewer. Replacing an index, importing old
/// events, or changing the source selection invalidates the stored peaks.
#[derive(Default)]
pub struct OverlapCache {
    history: Option<History>,
}

struct History {
    replica: String,
    position: i64,
    hosts: Option<String>,
    agent: Option<AgentKind>,
    days: u32,
    through: i64,
    daily: Vec<OverlapPoint>,
}

#[derive(Default)]
struct Active {
    last: HashMap<String, (i64, AgentKind)>,
    ordered: BTreeSet<(i64, String)>,
}

impl Active {
    fn record(&mut self, id: String, at: i64, agent: AgentKind) {
        if let Some((old, _)) = self.last.insert(id.clone(), (at, agent)) {
            self.ordered.remove(&(old, id.clone()));
        }
        self.ordered.insert((at, id));
    }

    fn count(&mut self, at: i64) -> usize {
        while self
            .ordered
            .first()
            .is_some_and(|(last, _)| *last <= at - WINDOW)
        {
            if let Some((_, id)) = self.ordered.pop_first() {
                self.last.remove(&id);
            }
        }
        self.last.len()
    }
}

impl Store {
    /// Count sessions in the trailing 15 minutes. Stream events by timestamp,
    /// retaining only active sessions and at most a year of daily peaks.
    pub fn overlap(
        &self,
        now: i64,
        days: u32,
        hosts: Option<&[&str]>,
        agent: Option<AgentKind>,
    ) -> Result<Overlap> {
        scan_overlap(
            &self.connection,
            first_day(now, days),
            now,
            host_filter(hosts).as_deref(),
            agent,
        )
    }

    /// An unchanged journal needs only the events since the previous sample,
    /// plus its 15-minute lookback to seed the active set. This also picks up
    /// previously indexed future timestamps as the clock reaches them.
    pub fn cached_overlap(
        &self,
        cache: &mut OverlapCache,
        now: i64,
        days: u32,
        hosts: Option<&[&str]>,
        agent: Option<AgentKind>,
    ) -> Result<Overlap> {
        let tx = self.connection.unchecked_transaction()?;
        // Read the journal cursor and chart under the same SQLite snapshot.
        let replica = crate::sync::storage::identity(&tx)?;
        let position = crate::sync::storage::journal_end(&tx)?;
        let hosts = host_filter(hosts);
        let first = first_day(now, days);
        let previous = cache.history.as_ref().filter(|h| {
            h.replica == replica
                && h.position == position
                && h.hosts == hosts
                && h.agent == agent
                && h.days == days
                && (first..=now).contains(&h.through)
        });
        let start = previous.map_or(first, |h| h.through);
        let mut chart = scan_overlap(&tx, start, now, hosts.as_deref(), agent)?;
        tx.commit()?;
        if let Some(history) = previous {
            // The first partial day meets the last cached day. Preserve its
            // earliest peak on ties, and drop days that rolled out of range.
            if let (Some(old), Some(new)) = (history.daily.last(), chart.daily.first_mut())
                && old.count >= new.count
            {
                *new = *old;
            }
            let boundary = start.div_euclid(DAY) * DAY;
            chart.daily.splice(
                0..0,
                history
                    .daily
                    .iter()
                    .filter(|p| p.at >= first && p.at < boundary)
                    .copied(),
            );
        }
        cache.history = Some(History {
            replica,
            position,
            hosts,
            agent,
            days,
            through: now,
            daily: chart.daily.clone(),
        });
        Ok(chart)
    }

    /// Session metadata for a chart point, independent of the recent-session listing.
    pub fn active_sessions(
        &self,
        at: i64,
        hosts: Option<&[&str]>,
        agent: Option<AgentKind>,
    ) -> Result<Vec<Session>> {
        let hosts = host_filter(hosts);
        let mut stmt = self.connection.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM sessions s JOIN hosts h USING (host_id)
             WHERE s.session_id IN (SELECT e.session_id {ACTIVITY})
             ORDER BY s.updated_at DESC, s.session_id LIMIT {MAX_LIMIT}"
        ))?;
        let rows = stmt.query_map(
            params![
                at.saturating_sub(WINDOW),
                at,
                hosts,
                agent.map(AgentKind::name)
            ],
            session,
        )?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.extend(row?);
        }
        Ok(sessions)
    }
}

fn first_day(now: i64, days: u32) -> i64 {
    (now.div_euclid(DAY) - i64::from(days.clamp(1, 365)) + 1) * DAY
}

fn host_filter(hosts: Option<&[&str]>) -> Option<String> {
    hosts.map(|ids| serde_json::json!(ids).to_string())
}

fn scan_overlap(
    connection: &Connection,
    first: i64,
    now: i64,
    hosts: Option<&str>,
    agent: Option<AgentKind>,
) -> Result<Overlap> {
    let mut stmt = connection.prepare_cached(&format!(
        "SELECT e.at, e.session_id, s.agent {ACTIVITY} ORDER BY e.at"
    ))?;
    let mut rows = stmt.query(params![
        first - WINDOW,
        now,
        hosts,
        agent.map(AgentKind::name)
    ])?;
    let mut row = rows.next()?;
    let mut active = Active::default();
    let mut daily = Vec::new();
    let mut start = first;
    while start <= now {
        let next_day = (start.div_euclid(DAY) + 1) * DAY;
        let mut peak = OverlapPoint {
            at: start,
            count: active.count(start),
        };
        while let Some(event) = row {
            let at: i64 = event.get(0)?;
            if at >= next_day {
                break;
            }
            let agent: String = event.get(2)?;
            if let Some(agent) = AgentKind::parse(&agent) {
                active.record(event.get(1)?, at, agent);
            }
            // Include activity carried in from before the first midnight.
            let at = at.max(start);
            let count = active.count(at);
            if count > peak.count {
                peak = OverlapPoint { at, count };
            }
            row = rows.next()?;
        }
        daily.push(peak);
        start = next_day;
    }
    Ok(Overlap {
        recent: OverlapPoint {
            at: now,
            count: active.count(now),
        },
        recent_by_agent: ActiveAgent::counts(active.last.values().map(|(_, agent)| *agent)),
        daily,
    })
}
