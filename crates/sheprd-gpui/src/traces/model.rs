//! What the Traces page shows, computed from a snapshot: the sessions a
//! filter keeps, their totals, and the chart's days. Pure, so the page draws
//! from prepared values and tests need no window.

use chrono::{Days, NaiveDate};
use sheprd_client::herdr::AgentKind;
use sheprd_control_protocol::protocol::app::{TaggedHost, TraceFilter};
use sheprd_traces::{Activity, Session};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// How far back the page looks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Range {
    Week,
    #[default]
    Month,
    Quarter,
    Year,
}

impl Range {
    pub(crate) const ALL: [Self; 4] = [Self::Week, Self::Month, Self::Quarter, Self::Year];

    pub(crate) fn days(self) -> u32 {
        match self {
            Self::Week => 7,
            Self::Month => 30,
            Self::Quarter => 90,
            Self::Year => 365,
        }
    }
}

/// What the person narrowed the page to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Filter {
    pub agent: Option<AgentKind>,
    pub range: Range,
    pub query: String,
    pub scope: TraceFilter,
}

impl Filter {
    /// Whether `session` was active in the range, is of the agent, and
    /// names the query in its title, folder, branch, model, or host.
    pub(crate) fn keeps(&self, session: &Session, now: i64, hosts: &[TaggedHost]) -> bool {
        if !self.scope.includes(&session.host_id, hosts) {
            return false;
        }
        let since = now - i64::from(self.range.days()) * DAY_MS;
        if session.updated_at.is_none_or(|updated| updated < since) {
            return false;
        }
        if self.agent.is_some_and(|agent| agent != session.agent) {
            return false;
        }
        self.matches_query(session)
    }

    pub(super) fn matches_query(&self, session: &Session) -> bool {
        let query = self.query.trim().to_lowercase();
        query.is_empty()
            || [
                session.title.as_deref(),
                session.cwd.as_deref(),
                session.branch.as_deref(),
                session.model.as_deref(),
                Some(session.host.as_str()),
                Some(session.agent.name()),
            ]
            .into_iter()
            .flatten()
            .any(|field| field.to_lowercase().contains(&query))
    }
}

/// The sessions a filter keeps, as the table lists them.
pub(crate) fn shown<'a>(
    sessions: &'a [Session],
    filter: &Filter,
    now: i64,
    hosts: &[TaggedHost],
) -> Vec<&'a Session> {
    sessions
        .iter()
        .filter(|session| filter.keeps(session, now, hosts))
        .collect()
}

/// Sums over the sessions shown.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Totals {
    pub sessions: usize,
    pub messages: i64,
    pub tool_calls: i64,
    pub errors: i64,
    pub tokens: i64,
    pub hosts: usize,
}

pub(crate) fn totals(sessions: &[&Session]) -> Totals {
    let mut hosts: Vec<&str> = sessions.iter().map(|s| s.host_id.as_str()).collect();
    hosts.sort_unstable();
    hosts.dedup();
    Totals {
        sessions: sessions.len(),
        messages: sessions.iter().map(|s| s.messages).sum(),
        tool_calls: sessions.iter().map(|s| s.tool_calls).sum(),
        errors: sessions.iter().map(|s| s.errors).sum(),
        tokens: sessions.iter().map(|s| s.tokens_in + s.tokens_out).sum(),
        hosts: hosts.len(),
    }
}

/// The table's agents plus those in the full activity history. A bounded
/// session listing must not remove older agents from the filters or chart.
pub(crate) fn agents(sessions: &[Session], activity: &[Activity]) -> Vec<AgentKind> {
    let mut counts: Vec<(AgentKind, i64)> = Vec::new();
    for session in sessions {
        match counts.iter_mut().find(|(agent, _)| *agent == session.agent) {
            Some((_, count)) => *count += session.messages.max(1),
            None => counts.push((session.agent, session.messages.max(1))),
        }
    }
    let recent: Vec<_> = counts.iter().map(|(agent, _)| *agent).collect();
    for entry in activity
        .iter()
        .filter(|entry| !recent.contains(&entry.agent))
    {
        match counts.iter_mut().find(|(agent, _)| *agent == entry.agent) {
            Some((_, count)) => *count = count.saturating_add(entry.messages.max(0)),
            None => counts.push((entry.agent, entry.messages.max(0))),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name().cmp(b.0.name())));
    counts.into_iter().map(|(agent, _)| agent).collect()
}

/// Messages per day for each agent over the range, ending today (UTC, as
/// the index keeps days).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Chart {
    pub days: Vec<NaiveDate>,
    /// The agents stacked, bottom first.
    pub agents: Vec<AgentKind>,
    /// `counts[day][agent]`, in the order of `days` and `agents`.
    pub counts: Vec<Vec<u32>>,
    pub peak: u32,
    pub total: u64,
}

pub(crate) fn chart(
    activity: &[Activity],
    agents: &[AgentKind],
    filter: &Filter,
    today: NaiveDate,
) -> Chart {
    let span = filter.range.days();
    let first = today
        .checked_sub_days(Days::new(u64::from(span - 1)))
        .unwrap_or(today);
    let days: Vec<NaiveDate> = first.iter_days().take(span as usize).collect();
    let stacked: Vec<AgentKind> = agents
        .iter()
        .copied()
        .filter(|agent| filter.agent.is_none_or(|only| only == *agent))
        .collect();
    let mut counts = vec![vec![0u32; stacked.len()]; days.len()];
    let mut total = 0u64;
    for entry in activity {
        let Ok(day) = NaiveDate::parse_from_str(&entry.day, "%Y-%m-%d") else {
            continue;
        };
        let Some(row) = day
            .signed_duration_since(first)
            .num_days()
            .try_into()
            .ok()
            .filter(|row: &usize| *row < days.len())
        else {
            continue;
        };
        let Some(column) = stacked.iter().position(|agent| *agent == entry.agent) else {
            continue;
        };
        let messages = u32::try_from(entry.messages).unwrap_or(u32::MAX);
        counts[row][column] = counts[row][column].saturating_add(messages);
        total += u64::from(messages);
    }
    let peak = counts
        .iter()
        .map(|day| day.iter().sum::<u32>())
        .max()
        .unwrap_or(0);
    Chart {
        days,
        agents: stacked,
        counts,
        peak,
        total,
    }
}

/// "12.3k", "4.5M": a count short enough for a tile.
pub(crate) fn compact(count: i64) -> String {
    let value = count as f64;
    match count.abs() {
        0..1_000 => count.to_string(),
        1_000..1_000_000 => format!("{:.1}k", value / 1_000.),
        1_000_000..1_000_000_000 => format!("{:.1}M", value / 1_000_000.),
        _ => format!("{:.1}B", value / 1_000_000_000.),
    }
}
