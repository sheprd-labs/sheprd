//! Drawing the Traces section: the header and search, the range and agent
//! filters, the totals, the chart, and the session table beside a selected
//! session's events, all from the window's snapshot.

use super::{
    Page, chart, detail,
    model::{self, Range, Totals},
    service::Status,
};
use crate::i18n::fl;
use crate::{SheprdWindow, config::corners, fonts::StyledFont, orchestrator::Look};
use gpui::{prelude::*, *};
use sheprd_client::herdr::AgentKind;
use sheprd_traces::Session;

/// The most rows the table draws; the filter narrows the rest.
const MAX_ROWS: usize = 200;
/// The events column's width beside the table.
const DETAIL_WIDTH: f32 = 440.;

impl SheprdWindow {
    /// What the page draws with: the window's theme and fonts.
    pub(super) fn traces_look(&self) -> Look {
        Look {
            theme: self.theme.clone(),
            ui: self.config.ui.clone(),
            mono: self.config.terminal.clone(),
        }
    }

    /// The section, across the window's body.
    pub(crate) fn render_traces_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.traces_look();
        let Some(page) = &self.traces.page else {
            return note(&look, fl!("traces-starting")).p_5().into_any_element();
        };
        let body = match &self.traces.status {
            Status::Starting => note(&look, fl!("traces-starting")).into_any_element(),
            Status::Failed(error) => {
                note(&look, fl!("traces-failed", error = error.as_str())).into_any_element()
            }
            Status::Ready(snapshot) => self
                .traces_body(&look, snapshot, page, cx)
                .into_any_element(),
        };
        div()
            .id("traces-section")
            .debug_selector(|| "traces-section".into())
            .size_full()
            .min_w_0()
            .bg(rgb(look.theme.background))
            .text_color(rgb(look.theme.foreground))
            .text_font(&look.ui)
            .text_size(px(look.ui.size))
            .child(
                div()
                    .id("traces-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&page.scroll)
                    .child(
                        div()
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .child(header(&look, page))
                            .child(replication(&look, &self.traces.peers))
                            .child(body),
                    ),
            )
            .into_any_element()
    }

    fn traces_body(
        &self,
        look: &Look,
        snapshot: &super::service::Snapshot,
        page: &Page,
        cx: &mut Context<Self>,
    ) -> Div {
        let now = chrono::Utc::now();
        let agents = model::agents(&snapshot.sessions, &snapshot.activity);
        let shown = model::shown(
            &snapshot.sessions,
            &page.filter,
            now.timestamp_millis(),
            &snapshot.hosts,
        );
        let totals = model::totals(&shown);
        let activity = if matches!(
            page.filter.scope,
            sheprd_control_protocol::protocol::app::TraceFilter::All
        ) {
            Some(Ok(snapshot.activity.as_slice()))
        } else {
            page.activity.as_ref().map(|result| result.as_deref())
        };
        let chart = match activity {
            Some(Ok(activity)) => chart::messages(
                look,
                &model::chart(activity, &agents, &page.filter, now.date_naive()),
                page.filter.range,
            )
            .into_any_element(),
            Some(Err(error)) => note(
                look,
                fl!("traces-activity-failed", error = error.to_string()),
            )
            .into_any_element(),
            None => note(look, fl!("traces-activity-loading")).into_any_element(),
        };
        let drilling = page.overlap.selection.is_some();
        let shown = if drilling {
            page.overlap
                .sessions
                .iter()
                .filter(|s| page.filter.matches_query(s))
                .collect()
        } else {
            shown
        };
        let selected = page
            .selected
            .as_deref()
            .and_then(|selected| shown.iter().copied().find(|session| session.id == selected));
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.trace_filters(look, page, &agents, cx))
            .child(self.trace_host_filters(look, &snapshot.hosts, &page.filter.scope, cx))
            .child(tiles(look, &totals))
            .child(self.overlap_panel(look, page, cx))
            .when(!drilling, |body| body.child(chart))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap_4()
                    .child(div().flex_1().flex_basis(px(740.)).min_w_0().child(
                        if page.overlap.loading_sessions() {
                            note(look, fl!("traces-activity-loading"))
                        } else {
                            self.trace_table(look, &shown, page, now, cx)
                        },
                    ))
                    .when_some(selected, |row, session| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(DETAIL_WIDTH))
                                .max_w_full()
                                .child(detail::pane(look, session, page, now, cx)),
                        )
                    }),
            )
    }

    fn trace_filters(
        &self,
        look: &Look,
        page: &Page,
        agents: &[AgentKind],
        cx: &mut Context<Self>,
    ) -> Div {
        let ranges = Range::ALL.into_iter().map(|range| {
            look.pill(
                format!("traces-range-{}", range.days()),
                page.filter.range == range,
            )
            .child(range_label(range))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(page) = &mut this.traces.page {
                    page.filter.range = range;
                    page.overlap.reset();
                    cx.notify();
                }
            }))
        });
        let choices = std::iter::once(None).chain(agents.iter().copied().map(Some));
        let agent_pills = choices.map(|agent| {
            let on = page.filter.agent == agent;
            let name = agent.map_or("all", AgentKind::name);
            look.pill(format!("traces-agent-{name}"), on)
                .when_some(agent, |pill, agent| {
                    pill.child(look.dot(agent_color(look, agent), 7.))
                })
                .child(match agent {
                    Some(agent) => agent_label(agent),
                    None => fl!("traces-agent-all"),
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(page) = &mut this.traces.page {
                        page.filter.agent = agent;
                        page.overlap.reset();
                        cx.notify();
                    }
                }))
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(ranges)
            .child(div().w(px(12.)))
            .children(agent_pills)
    }

    fn trace_table(
        &self,
        look: &Look,
        shown: &[&Session],
        page: &Page,
        now: chrono::DateTime<chrono::Utc>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &look.theme;
        let panel = div()
            .debug_selector(|| "traces-table".into())
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .overflow_hidden()
            .flex()
            .flex_col();
        if shown.is_empty() {
            return panel.child(div().p_4().child(note(look, fl!("traces-none"))));
        }
        let heading = row(look)
            .text_size(look.small())
            .text_color(rgb(theme.subtext()))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(cell_when(fl!("traces-column-updated")))
            .child(cell_title(fl!("traces-column-session")))
            .child(cell_fixed(110., fl!("traces-column-host")))
            .child(cell_fixed(150., fl!("traces-column-model")))
            .child(cell_number(fl!("traces-column-messages")))
            .child(cell_number(fl!("traces-column-tools")))
            .child(cell_number(fl!("traces-column-tokens")));
        let rows = shown.iter().take(MAX_ROWS).map(|session| {
            let selected = page.selected.as_deref() == Some(session.id.as_str());
            let session_id = session.id.clone();
            let hover = theme.active;
            row(look)
                .id(SharedString::from(format!("traces-row-{}", session.id)))
                .cursor_pointer()
                .when(selected, |row| row.bg(rgb(theme.primary_wash())))
                .when(!selected, |row| {
                    row.hover(move |style| style.bg(rgb(hover)))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_trace(session_id.clone(), cx);
                }))
                .child(cell_when(when(session.updated_at, now)).text_color(rgb(theme.subtext())))
                .child(
                    cell_title(String::new())
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(look.dot(agent_color(look, session.agent), 7.))
                        .child(div().min_w_0().truncate().child(title(session))),
                )
                .child(cell_fixed(110., session.host.clone()).text_color(rgb(theme.subtext())))
                .child(
                    cell_fixed(150., session.model.clone().unwrap_or_default())
                        .text_color(rgb(theme.subtext())),
                )
                .child(cell_number(session.messages.to_string()))
                .child(cell_number(session.tool_calls.to_string()))
                .child(cell_number(model::compact(
                    session.tokens_in + session.tokens_out,
                )))
        });
        let hidden = shown.len().saturating_sub(MAX_ROWS);
        let table = panel
            .min_w(px(740.))
            .child(heading)
            .children(rows)
            .when(hidden > 0, |panel| {
                panel.child(
                    div()
                        .p_3()
                        .text_size(look.small())
                        .text_color(rgb(theme.muted))
                        .child(fl!("traces-more", count = hidden)),
                )
            });
        div().min_w_0().child(
            div()
                .id("traces-table-scroll")
                .debug_selector(|| "traces-table-viewport".into())
                .overflow_x_scroll()
                .child(table),
        )
    }
}

fn header(look: &Look, page: &Page) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_end()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w(px(220.))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(look.ui.size + 7.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(fl!("traces-title")),
                )
                .child(
                    div()
                        .text_color(rgb(look.theme.subtext()))
                        .child(fl!("traces-subtitle")),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(160.))
                .max_w(px(320.))
                .child(page.search.clone()),
        )
}

fn replication(look: &Look, peers: &[super::replication::Report]) -> Div {
    use super::replication::Phase;
    div()
        .debug_selector(|| "traces-replication".into())
        .flex()
        .flex_col()
        .gap_1()
        .text_size(look.small())
        .text_color(rgb(look.theme.subtext()))
        .child(fl!("traces-replication-summary", count = peers.len()))
        .children(peers.iter().map(|peer| {
            let status = match &peer.state {
                Phase::Waiting => fl!("traces-replication-waiting"),
                Phase::Syncing => fl!("traces-replication-syncing"),
                Phase::Ready => fl!("traces-replication-ready"),
                Phase::Failed { error } => fl!(
                    "traces-replication-failed",
                    error = crate::notifications::safe_text(&error.to_string(), 512)
                ),
            };
            div().child(fl!(
                "traces-replication-peer",
                host = peer.host.as_str(),
                status = status
            ))
        }))
}

fn tiles(look: &Look, totals: &Totals) -> Div {
    let theme = &look.theme;
    let cells = [
        (
            fl!("traces-tile-sessions"),
            totals.sessions.to_string(),
            fl!("traces-tile-hosts", count = totals.hosts),
        ),
        (
            fl!("traces-tile-messages"),
            model::compact(totals.messages),
            String::new(),
        ),
        (
            fl!("traces-tile-tools"),
            model::compact(totals.tool_calls),
            fl!("traces-tile-errors", count = totals.errors),
        ),
        (
            fl!("traces-tile-tokens"),
            model::compact(totals.tokens),
            String::new(),
        ),
    ];
    div()
        .debug_selector(|| "traces-totals".into())
        .rounded(px(corners::PANEL))
        .border_1()
        .border_color(rgb(theme.active))
        .bg(rgb(theme.surface))
        .overflow_hidden()
        .flex()
        .flex_wrap()
        .children(
            cells
                .into_iter()
                .enumerate()
                .map(|(index, (title, value, detail))| {
                    div()
                        .flex_1()
                        .min_w(px(100.))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when(index > 0, |cell| {
                            cell.border_l_1().border_color(rgb(theme.active))
                        })
                        .child(
                            div()
                                .text_size(look.small())
                                .text_color(rgb(theme.subtext()))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(look.ui.size + 9.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(value),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(look.small())
                                .text_color(rgb(theme.muted))
                                .child(detail),
                        )
                }),
        )
}

fn row(look: &Look) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py(px(6.))
        .min_w_0()
        .text_size(look.small())
}

fn cell_when(text: String) -> Div {
    div().flex_none().w(px(70.)).truncate().child(text)
}

fn cell_title(text: String) -> Div {
    div().flex_1().min_w(px(120.)).truncate().child(text)
}

fn cell_fixed(width: f32, text: String) -> Div {
    div().flex_none().w(px(width)).truncate().child(text)
}

fn cell_number(text: String) -> Div {
    div()
        .flex_none()
        .w(px(56.))
        .flex()
        .justify_end()
        .whitespace_nowrap()
        .child(text)
}

pub(super) fn note(look: &Look, text: String) -> Div {
    div()
        .debug_selector(|| "traces-note".into())
        .text_color(rgb(look.theme.muted))
        .child(text)
}

pub(super) fn title(session: &Session) -> String {
    session
        .title
        .clone()
        .unwrap_or_else(|| fl!("traces-untitled"))
}

/// "5 min ago", "yesterday".
pub(super) fn when(at: Option<i64>, now: chrono::DateTime<chrono::Utc>) -> String {
    at.map(|at| crate::ago::text(crate::ago::since(at / 1000, now.timestamp())))
        .unwrap_or_default()
}

/// Each agent keeps its colour whatever the filter, from the theme's own
/// palette.
pub(super) fn agent_color(look: &Look, agent: AgentKind) -> u32 {
    let index = match agent {
        AgentKind::Claude => 3,
        AgentKind::Codex => 4,
        AgentKind::Pi => 5,
        AgentKind::Omp => 6,
        _ => 2,
    };
    look.hue(index)
}

pub(super) fn agent_label(agent: AgentKind) -> String {
    match agent {
        AgentKind::Claude => fl!("traces-agent-claude"),
        AgentKind::Codex => fl!("traces-agent-codex"),
        AgentKind::Pi => fl!("traces-agent-pi"),
        AgentKind::Omp => fl!("traces-agent-omp"),
        other => other.name().to_owned(),
    }
}

fn range_label(range: Range) -> String {
    match range {
        Range::Week => fl!("traces-range-week"),
        Range::Month => fl!("traces-range-month"),
        Range::Quarter => fl!("traces-range-quarter"),
        Range::Year => fl!("traces-range-year"),
    }
}
