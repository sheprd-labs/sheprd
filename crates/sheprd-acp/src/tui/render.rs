//! Draws the view.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use sheprd_chat_protocol::{PermissionKind, Status};

use super::{lines, view::View};

mod status;
mod welcome;

const COMPOSER_ROWS: usize = 8;

pub(crate) fn draw(frame: &mut Frame<'_>, view: &mut View, tick: usize) {
    let area = frame.area();
    let width = usize::from(area.width);
    let plan = lines::plan(&view.transcript.plan, width);
    let plan_height = plan.len().min(6);
    let queued = view.transcript.queued.len().min(3);
    let (bottom_height, bottom) = if !view.transcript.permissions.is_empty() {
        permission_height(view, width)
    } else if view.settings.is_some() {
        (view.setting_rows() + 2, Bottom::Settings)
    } else {
        composer_height(view, width)
    };
    let completions = if bottom == Bottom::Composer {
        view.completions().len()
    } else {
        0
    };
    let [
        body,
        plan_area,
        queue_area,
        completion_area,
        bottom_area,
        footer,
    ] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(height(plan_height)),
        Constraint::Length(height(queued)),
        Constraint::Length(height(completions)),
        Constraint::Length(height(bottom_height)),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_body(frame, view, body, tick);
    frame.render_widget(
        Paragraph::new(plan.into_iter().take(plan_height).collect::<Vec<_>>()),
        plan_area,
    );
    let queue: Vec<Line<'_>> = view
        .transcript
        .queued
        .iter()
        .take(3)
        .map(|queued| {
            let first = queued.text.lines().next().unwrap_or_default();
            let state = if queued.steered {
                "steering"
            } else if queued.held {
                "held"
            } else {
                "queued"
            };
            Line::styled(
                format!("↳ {state}: {first}"),
                Style::default().fg(Color::DarkGray),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(queue), queue_area);
    draw_completions(frame, view, completion_area);
    match bottom {
        Bottom::Composer => draw_composer(frame, view, bottom_area),
        Bottom::Permission => draw_permission(frame, view, bottom_area),
        Bottom::Settings => draw_settings(frame, view, bottom_area),
    }
    let bar = status::line(view, tick, width, bottom == Bottom::Settings);
    frame.render_widget(Paragraph::new(bar), footer);
}

fn height(rows: usize) -> u16 {
    u16::try_from(rows).unwrap_or(u16::MAX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bottom {
    Composer,
    Permission,
    Settings,
}

fn draw_completions(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let rows: Vec<Line<'_>> = view
        .completions()
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let style = if index == view.completion {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(format!(" /{} ", command.name), style.fg(Color::Cyan)),
                Span::styled(
                    format!(" {}", command.description),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), area);
}

fn draw_settings(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let selected = view.settings.unwrap_or(0);
    // While the chat may switch agents, the agent comes first.
    let agent = view
        .transcript
        .harnesses
        .as_ref()
        .map(|_| ("Agent".to_owned(), view.transcript.agent.clone()));
    let options = view
        .transcript
        .options
        .iter()
        .map(|option| (option.name.clone(), option.current_name()));
    let rows: Vec<Line<'_>> = agent
        .into_iter()
        .chain(options)
        .enumerate()
        .map(|(index, (name, current))| {
            let marker = if index == selected { "› " } else { "  " };
            let value = Style::default()
                .fg(Color::Cyan)
                .add_modifier(if index == selected {
                    Modifier::REVERSED
                } else {
                    Modifier::empty()
                });
            Line::from(vec![
                Span::raw(marker),
                Span::styled(format!("{name}: "), Style::default()),
                Span::styled(format!(" ‹ {current} › "), value),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);
}

fn draw_body(frame: &mut Frame<'_>, view: &mut View, area: Rect, tick: usize) {
    if view.transcript.entries.is_empty() && !view.transcript.truncated {
        welcome::draw(frame, view, area, tick);
        return;
    }
    let lines = view.lines(usize::from(area.width));
    let rows = usize::from(area.height);
    let most = lines.len().saturating_sub(rows);
    view.scroll = view.scroll.min(most);
    let start = most - view.scroll;
    let visible: Vec<Line<'static>> = lines.into_iter().skip(start).take(rows).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn composer_height(view: &View, width: usize) -> (usize, Bottom) {
    let (rows, _) = view.composer.layout(width.saturating_sub(PROMPT_MARK));
    // A rule above the prompt's rows sets it apart from the transcript.
    (rows.len().clamp(1, COMPOSER_ROWS) + 1, Bottom::Composer)
}

/// Columns of the `❯ ` mark before the prompt's text.
const PROMPT_MARK: usize = 2;

fn draw_composer(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let [rule, rows] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let dim = Style::default().fg(Color::DarkGray);
    let fill = "─".repeat(usize::from(rule.width));
    // Files the prompt carries name themselves on the rule.
    let rule_line = if view.attachments.is_empty() {
        Line::styled(fill, dim)
    } else {
        let names: Vec<&str> = view.attachments.iter().map(|file| file.name()).collect();
        Line::from(vec![
            Span::styled("─ ", dim),
            Span::styled(
                format!("\u{1f4ce} {} ", names.join(" \u{b7} ")),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(fill, dim),
        ])
    };
    frame.render_widget(Paragraph::new(rule_line), rule);
    let [mark, inner] =
        Layout::horizontal([Constraint::Length(height(PROMPT_MARK)), Constraint::Min(1)])
            .areas(rows);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "❯ ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )),
        mark,
    );
    if view.composer.text.is_empty() {
        let hint = match view.transcript.status {
            Status::Working => "Type to queue a prompt…".to_owned(),
            Status::Failed { .. } | Status::Exited { .. } => {
                "The agent is not running. Ctrl+D quits.".to_owned()
            }
            _ => format!("Ask {}…", view.transcript.agent),
        };
        frame.render_widget(
            Paragraph::new(Line::styled(hint, Style::default().fg(Color::DarkGray))),
            inner,
        );
        frame.set_cursor_position(Position::new(inner.x, inner.y));
        return;
    }
    let (rows, (row, column)) = view.composer.layout(usize::from(inner.width));
    let visible = usize::from(inner.height).max(1);
    let first = row.saturating_sub(visible - 1);
    let text: Vec<Line<'_>> = rows
        .into_iter()
        .skip(first)
        .take(visible)
        .map(Line::from)
        .collect();
    frame.render_widget(Paragraph::new(text), inner);
    let x = inner.x.saturating_add(height(column));
    let y = inner.y.saturating_add(height(row - first));
    frame.set_cursor_position(Position::new(x, y));
}

fn permission_height(view: &View, width: usize) -> (usize, Bottom) {
    let Some(permission) = view.transcript.permissions.first() else {
        return (0, Bottom::Permission);
    };
    let title = lines::wrap(&permission.title, width.saturating_sub(2))
        .len()
        .min(3);
    (title + permission.options.len() + 2, Bottom::Permission)
}

fn draw_permission(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let Some(permission) = view.transcript.permissions.first() else {
        return;
    };
    let more = view.transcript.permissions.len() - 1;
    let title = if more > 0 {
        format!(" Permission (+{more} more) ")
    } else {
        " Permission ".to_owned()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut text: Vec<Line<'_>> = lines::wrap(&permission.title, usize::from(inner.width))
        .into_iter()
        .take(3)
        .map(|line| Line::styled(line, Style::default().add_modifier(Modifier::BOLD)))
        .collect();
    for (index, option) in permission.options.iter().enumerate() {
        let color = match option.kind {
            PermissionKind::AllowOnce | PermissionKind::AllowAlways => Color::Green,
            PermissionKind::RejectOnce | PermissionKind::RejectAlways => Color::Red,
            PermissionKind::Other => Color::Reset,
        };
        let mut style = Style::default().fg(color);
        if index == view.choice {
            style = style.add_modifier(Modifier::REVERSED);
        }
        text.push(Line::from(vec![
            Span::raw(if index == view.choice { "› " } else { "  " }),
            Span::styled(format!("{}. {}", index + 1, option.name), style),
        ]));
    }
    frame.render_widget(Paragraph::new(text), inner);
}

#[cfg(test)]
mod tests;
