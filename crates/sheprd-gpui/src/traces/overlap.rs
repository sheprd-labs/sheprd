//! A window reads only its selected range and hosts. The recent count ages
//! even when every agent is quiet.
use super::{Page, Service, view};
use crate::{SheprdWindow, config::corners, i18n::fl, orchestrator::Look};
use gpui::{prelude::*, *};
use sheprd_control_protocol::protocol::app::TraceFilter;
use sheprd_traces::{Overlap, OverlapCache, OverlapPoint, Session, Store};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Selection {
    Recent,
    Peak(i64),
}

fn day(at: i64) -> i64 {
    at.div_euclid(86_400_000)
}

fn date(at: i64, format: &str) -> String {
    chrono::DateTime::from_timestamp_millis(at)
        .map(|t| t.format(format).to_string())
        .unwrap_or_default()
}

#[derive(Default)]
pub(super) struct Panel {
    chart: Option<Overlap>,
    cache: OverlapCache,
    pub(super) selection: Option<Selection>,
    pub(super) sessions: Vec<Session>,
    error: Option<sheprd_traces::Error>,
    read: Option<Task<()>>,
    version: u64,
    selection_dirty: bool,
    refreshed: Option<Instant>,
}

impl Panel {
    pub(super) fn reset(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.chart = None;
        self.cache = OverlapCache::default();
        self.selection_dirty = false;
        self.selection = None;
        self.sessions.clear();
        self.error = None;
        self.refreshed = None;
    }

    pub(super) fn loading_sessions(&self) -> bool {
        self.selection.is_some() && self.selection_dirty && self.error.is_none()
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub(super) fn fixture(&mut self, chart: Overlap) {
        self.chart = Some(chart);
        self.refreshed = Some(Instant::now());
    }

    fn pick(&mut self, selection: Option<Selection>) {
        self.selection = selection;
        self.sessions.clear();
        self.error = None;
        self.version = self.version.wrapping_add(1);
        self.selection_dirty = true;
    }
}

impl SheprdWindow {
    pub(super) fn poll_overlap(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.traces.page.as_ref() else {
            return;
        };
        let p = &page.overlap;
        if p.read.is_some()
            || (!p.selection_dirty
                && p.refreshed
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(30)))
        {
            return;
        }
        let Some(service) = cx.try_global::<Service>() else {
            return;
        };
        let path = service.index_path().map(ToOwned::to_owned);
        let scope = &page.filter.scope;
        let hosts = self.trace_hosts();
        let ids = (!matches!(scope, TraceFilter::All)).then(|| {
            hosts
                .iter()
                .filter(|h| scope.includes(&h.id, hosts))
                .map(|h| h.id.clone())
                .collect::<Vec<_>>()
        });
        let days = page.filter.range.days();
        let agent = page.filter.agent;
        let selection = p.selection;
        let version = p.version;
        // On a click, query membership against the chart already displayed.
        // The regular timed refresh picks up newer history independently.
        let displayed = p.chart.as_ref().filter(|_| p.selection_dirty).cloned();
        let refresh_chart = displayed.is_none();
        let Some(page) = self.traces.page.as_mut() else {
            return;
        };
        let mut cache = std::mem::take(&mut page.overlap.cache);
        let read = cx.background_executor().spawn(async move {
            let store = Store::open(&path.ok_or(sheprd_traces::Error::NoStateDirectory)?)?;
            let ids = ids
                .as_ref()
                .map(|ids| ids.iter().map(String::as_str).collect::<Vec<_>>());
            let chart = match displayed {
                Some(chart) => chart,
                None => store.cached_overlap(
                    &mut cache,
                    chrono::Utc::now().timestamp_millis(),
                    days,
                    ids.as_deref(),
                    agent,
                )?,
            };
            let at = selection.and_then(|s| selected_at(&chart, s));
            let sessions = match at {
                Some(at) => store.active_sessions(at, ids.as_deref(), agent)?,
                None => Vec::new(),
            };
            Ok((chart, sessions, cache))
        });
        let task = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                let Some(page) = &mut this.traces.page else {
                    return;
                };
                let p = &mut page.overlap;
                p.read = None;
                if p.version == version {
                    p.selection_dirty = false;
                    if refresh_chart || result.is_err() {
                        p.refreshed = Some(Instant::now());
                    }
                    match result {
                        Ok((chart, sessions, cache)) => {
                            if p.selection
                                .is_some_and(|s| selected_at(&chart, s).is_none())
                            {
                                p.selection = None;
                            }
                            p.cache = cache;
                            p.chart = Some(chart);
                            p.sessions = sessions;
                            p.error = None;
                        }
                        Err(error) => p.error = Some(error),
                    }
                }
                cx.notify();
            });
        });
        if let Some(page) = &mut self.traces.page {
            page.overlap.read = Some(task);
        }
    }

    fn pick_overlap(&mut self, selection: Option<Selection>, cx: &mut Context<Self>) {
        if let Some(page) = &mut self.traces.page {
            page.overlap.pick(selection);
            page.selected = None;
            page.detail = None;
            page.load = None;
        }
        self.poll_overlap(cx);
        cx.notify();
    }

    pub(super) fn overlap_panel(&self, look: &Look, page: &Page, cx: &mut Context<Self>) -> Div {
        let p = &page.overlap;
        let panel = div()
            .debug_selector(|| "traces-overlap".into())
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(look.theme.active))
            .bg(rgb(look.theme.surface))
            .p_4()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("traces-overlap-title")),
            );
        if let Some(error) = &p.error {
            return panel.child(view::note(
                look,
                fl!("traces-activity-failed", error = error.to_string()),
            ));
        }
        let Some(chart) = &p.chart else {
            return panel.child(view::note(look, fl!("traces-activity-loading")));
        };
        let Some(peak) = chart.daily.iter().rev().max_by_key(|v| v.count) else {
            return panel;
        };
        let recent = Selection::Recent;
        let strongest = Selection::Peak(peak.at);
        let summary = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                look.pill("traces-recent", p.selection == Some(recent))
                    .child(fl!("traces-overlap-recent", count = chart.recent.count))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.pick_overlap(Some(recent), cx)),
                    ),
            )
            .children(chart.recent_by_agent.iter().map(|entry| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(look.small())
                    .child(look.dot(view::agent_color(look, entry.agent), 7.))
                    .child(fl!(
                        "traces-overlap-agent",
                        count = entry.count,
                        agent = view::agent_label(entry.agent)
                    ))
            }))
            .when(peak.count > 0, |row| {
                row.child(
                    look.pill("traces-peak", p.selection == Some(strongest))
                        .child(point_label(*peak))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick_overlap(Some(strongest), cx)
                        })),
                )
            });
        let bars =
            chart.daily.iter().map(|point| {
                let selection = Selection::Peak(point.at);
                let selected = p.selection == Some(selection);
                let (foreground, surface) = (look.theme.foreground, look.theme.surface);
                let point = *point;
                div()
                    .id(("overlap-day", day(point.at) as usize))
                    .debug_selector(move || format!("traces-overlap-day-{}", day(point.at)))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .tooltip(move |_, cx| {
                        cx.new(|_| crate::usage::Hint {
                            text: point_label(point).into(),
                            foreground,
                            surface,
                        })
                        .into()
                    })
                    .when(selected, |bar| bar.bg(rgb(look.theme.primary_wash())))
                    .when(point.count > 0, |bar| {
                        bar.cursor_pointer()
                            .hover(|s| s.bg(rgb(look.theme.active)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.pick_overlap(Some(selection), cx)
                            }))
                    })
            });
        let axis = div()
            .flex()
            .justify_between()
            .text_size(look.small())
            .text_color(rgb(look.theme.muted))
            .children(
                chart
                    .daily
                    .first()
                    .into_iter()
                    .chain(chart.daily.last())
                    .map(|p| date(p.at, "%b %-d")),
            );
        panel
            .child(summary)
            .child(view::note(look, fl!("traces-overlap-rule")).text_size(look.small()))
            .child(
                div()
                    .relative()
                    .flex()
                    .h(px(82.))
                    .border_b_1()
                    .border_color(rgb(look.theme.active))
                    .children(bars)
                    .child(line(&chart.daily, peak.count.max(1), look.theme.primary())),
            )
            .child(axis)
            .when_some(p.selection, |panel, selection| {
                panel.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .items_center()
                        .when_some(selected_at(chart, selection), |row, at| {
                            row.child(fl!(
                                "traces-overlap-selection",
                                time = date(at, "%b %-d %H:%M UTC")
                            ))
                        })
                        .child(
                            look.pill("traces-clear-overlap", false)
                                .child(fl!("traces-overlap-clear"))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.pick_overlap(None, cx)),
                                ),
                        ),
                )
            })
    }
}

fn point_label(point: OverlapPoint) -> String {
    fl!(
        "traces-overlap-peak",
        count = point.count,
        time = date(point.at, "%b %-d %H:%M UTC")
    )
}

fn selected_at(chart: &Overlap, selection: Selection) -> Option<i64> {
    match selection {
        Selection::Recent => Some(chart.recent.at),
        // A replicated event may move today's peak. Keep inspecting the
        // clicked instant until its day leaves the chart or the user exits.
        Selection::Peak(at) => chart
            .daily
            .iter()
            .any(|p| day(p.at) == day(at))
            .then_some(at),
    }
}

#[cfg(test)]
mod tests;

/// Put points at the centers of the same equal-width slots used for picking.
fn line(points: &[OverlapPoint], peak: usize, color: u32) -> impl IntoElement {
    let counts: Vec<_> = points.iter().map(|p| p.count).collect();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let slot = f32::from(bounds.size.width) / counts.len().max(1) as f32;
            let height = f32::from(bounds.size.height) - 4.;
            let mut path = PathBuilder::stroke(px(2.));
            for (i, n) in counts.iter().enumerate() {
                let pos = point(
                    bounds.left() + px(slot * (i as f32 + 0.5)),
                    bounds.bottom() - px(2. + height * *n as f32 / peak as f32),
                );
                if i == 0 {
                    path.move_to(pos);
                } else {
                    path.line_to(pos);
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(color));
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}
