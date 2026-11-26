//! What the body shows before the chat has any entries: the agent and its
//! state, where it runs, what keys do, and whether another client shows
//! this chat.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::status;
use crate::tui::view::View;

/// `path` with the home directory written as `~`.
fn tilde(path: &str, home: Option<&str>) -> String {
    match home.filter(|home| !home.is_empty()) {
        Some(home) if path == home => "~".to_owned(),
        Some(home) => path
            .strip_prefix(home)
            .filter(|rest| rest.starts_with('/'))
            .map_or_else(|| path.to_owned(), |rest| format!("~{rest}")),
        None => path.to_owned(),
    }
}

/// Key hints for what this chat offers, as many as fit in `width`.
fn tips(view: &View, width: usize) -> Line<'static> {
    let transcript = &view.transcript;
    let mut pairs = vec![("Enter", "send")];
    if !transcript.commands.is_empty() {
        pairs.push(("/", "commands"));
    }
    if transcript.modes.is_some()
        || transcript
            .option(sheprd_chat_protocol::OptionCategory::Mode)
            .is_some()
    {
        pairs.push(("Shift+Tab", "mode"));
    }
    if view.setting_rows() > 0 {
        pairs.push(("Ctrl+O", "settings"));
    }
    pairs.push(("Drop files", "to attach"));
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for (key, label) in pairs {
        let gap = if spans.is_empty() { 0 } else { 3 };
        let more = gap + key.chars().count() + 1 + label.chars().count();
        if used + more > width {
            break;
        }
        if gap > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::raw(key));
        spans.push(Span::styled(format!(" {label}"), dim));
        used += more;
    }
    Line::from(spans)
}

pub(super) fn lines(view: &View, tick: usize, width: usize) -> Vec<Line<'static>> {
    let transcript = &view.transcript;
    let dim = Style::default().fg(Color::DarkGray);
    let (state, color) = status::state(transcript, tick);
    let name = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD);
    let mut out = vec![Line::from(vec![
        Span::styled("◇ ", name),
        Span::styled(transcript.agent.clone(), name),
        Span::raw("  "),
        Span::styled(state, Style::default().fg(color)),
    ])];
    let settings = status::settings(transcript);
    if !settings.is_empty() {
        out.push(Line::styled(settings.join(" · "), dim));
    }
    if let Some(cwd) = transcript.session.as_ref().and_then(|s| s.cwd.as_deref()) {
        let home = std::env::var("HOME").ok();
        out.push(Line::default());
        out.push(Line::raw(tilde(cwd, home.as_deref())));
    }
    out.push(Line::default());
    out.push(tips(view, width));
    out.push(Line::default());
    out.push(match status::attached(transcript) {
        Some(attached) => Line::from(vec![
            Span::styled("● ", Style::default().fg(Color::Cyan)),
            Span::styled(attached, dim),
        ]),
        None => Line::styled(
            "○ Sheprd's chat is not attached. Open this pane in Sheprd for images and diffs.",
            dim,
        ),
    });
    out
}

pub(super) fn draw(frame: &mut Frame<'_>, view: &View, area: Rect, tick: usize) {
    let lines = lines(view, tick, usize::from(area.width));
    let rows = usize::from(area.height);
    let top = rows.saturating_sub(lines.len()) / 2;
    let mut shown: Vec<Line<'static>> = vec![Line::default(); top];
    shown.extend(lines.into_iter().take(rows.saturating_sub(top)));
    frame.render_widget(Paragraph::new(shown).alignment(Alignment::Center), area);
}

#[cfg(test)]
mod tests {
    use super::tilde;

    #[test]
    fn home_is_written_as_a_tilde() {
        assert_eq!(tilde("/Users/a/code", Some("/Users/a")), "~/code");
        assert_eq!(tilde("/Users/a", Some("/Users/a")), "~");
        assert_eq!(tilde("/Users/ab/code", Some("/Users/a")), "/Users/ab/code");
        assert_eq!(tilde("/srv/x", None), "/srv/x");
    }
}
