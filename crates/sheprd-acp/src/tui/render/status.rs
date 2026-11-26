//! The status bar along the bottom: the agent and its state as colored
//! blocks, its settings and context after them, and key hints or the last
//! flash on the right. What does not fit goes, least useful first.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use sheprd_chat_protocol::{OptionCategory, Status, Transcript, display};

use crate::tui::view::View;

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

const COMPOSER_HINTS: [&str; 4] = [
    "Enter send · Alt+Enter newline · Esc cancel · Shift+Tab mode · Ctrl+O settings · Ctrl+D quit",
    "Enter send · Esc cancel · Ctrl+O settings · Ctrl+D quit",
    "Enter send · Ctrl+D quit",
    "Ctrl+D quit",
];
const SETTINGS_HINTS: [&str; 2] = [
    "↑/↓ setting · ←/→ change · Esc close",
    "←/→ change · Esc close",
];

/// The agent's state in words, and its color.
pub(super) fn state(transcript: &Transcript, tick: usize) -> (String, Color) {
    let spinner = SPINNER[tick % SPINNER.len()];
    match &transcript.status {
        _ if transcript.is_blocked() => ("waiting for you".to_owned(), Color::Magenta),
        Status::Starting => (format!("{spinner} starting"), Color::Yellow),
        Status::Idle => ("ready".to_owned(), Color::Green),
        Status::Working => {
            let elapsed = transcript.turn.map(|turn| {
                display::duration(crate::session::now_ms().saturating_sub(turn.started_ms))
            });
            let elapsed = elapsed
                .filter(|elapsed| !elapsed.ends_with("ms"))
                .map_or_else(String::new, |elapsed| format!(" {elapsed}"));
            (format!("{spinner} working{elapsed}"), Color::Yellow)
        }
        Status::Failed { .. } => ("failed".to_owned(), Color::Red),
        Status::Exited { .. } => ("exited".to_owned(), Color::Red),
    }
}

/// The names of the settings the agent offers, model first; its older modes
/// only when it offers no mode setting.
pub(super) fn settings(transcript: &Transcript) -> Vec<String> {
    let mut names: Vec<String> = [
        OptionCategory::Model,
        OptionCategory::ThoughtLevel,
        OptionCategory::Mode,
    ]
    .into_iter()
    .filter_map(|category| transcript.option(category))
    .map(|option| option.current_name())
    .collect();
    if transcript.option(OptionCategory::Mode).is_none()
        && let Some(mode) = transcript.modes.as_ref().and_then(|modes| modes.current())
    {
        names.push(mode.name.clone());
    }
    names
}

/// "1 client attached", or `None` while nothing else shows this chat.
pub(super) fn attached(transcript: &Transcript) -> Option<String> {
    match transcript.clients {
        0 => None,
        1 => Some("1 client attached".to_owned()),
        count => Some(format!("{count} clients attached")),
    }
}

/// An item of the bar, and how soon it goes when space runs out: higher
/// goes first.
struct Item {
    drop: u8,
    spans: Vec<Span<'static>>,
}

fn items(transcript: &Transcript) -> Vec<Item> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut items: Vec<Item> = settings(transcript)
        .into_iter()
        .enumerate()
        .map(|(index, name)| Item {
            drop: if index == 0 { 2 } else { 3 },
            spans: vec![Span::raw(name)],
        })
        .collect();
    if transcript.auto_accept {
        items.push(Item {
            drop: 1,
            spans: vec![Span::styled(
                "auto-accepting",
                Style::default().fg(Color::Yellow),
            )],
        });
    }
    if let Some(usage) = transcript.usage.as_ref().filter(|usage| usage.size > 0) {
        let percent = usage.percent();
        let color = match percent {
            90.. => Color::Red,
            75..90 => Color::Yellow,
            _ => Color::DarkGray,
        };
        items.push(Item {
            drop: 2,
            spans: vec![Span::styled(
                format!(
                    "{}/{} context ({percent}%)",
                    display::tokens(usage.used),
                    display::tokens(usage.size)
                ),
                Style::default().fg(color),
            )],
        });
        if let Some(cost) = usage.cost.as_ref().filter(|cost| cost.micros > 0) {
            items.push(Item {
                drop: 5,
                spans: vec![Span::styled(display::cost(cost), dim)],
            });
        }
    }
    if let Some(attached) = attached(transcript) {
        items.push(Item {
            drop: 4,
            spans: vec![Span::styled(attached, Style::default().fg(Color::Cyan))],
        });
    }
    if let Some(title) = transcript.session.as_ref().and_then(|s| s.title.clone()) {
        items.push(Item {
            drop: 6,
            spans: vec![Span::styled(title, dim)],
        });
    }
    items
}

fn width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// Every item, a separator before each.
fn joined(items: &[Item]) -> Vec<Span<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = Vec::new();
    for (index, item) in items.iter().enumerate() {
        spans.push(Span::styled(if index == 0 { " " } else { " · " }, dim));
        spans.extend(item.spans.iter().cloned());
    }
    spans
}

pub(super) fn line(view: &View, tick: usize, columns: usize, settings: bool) -> Line<'static> {
    let transcript = &view.transcript;
    let (status, color) = state(transcript, tick);
    let blocks = vec![
        Span::styled(
            format!(" {} ", transcript.agent),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
        ),
        Span::styled(
            format!(" {status} "),
            Style::default().fg(color).add_modifier(Modifier::REVERSED),
        ),
    ];
    let hints: &[&str] = if settings {
        &SETTINGS_HINTS
    } else {
        &COMPOSER_HINTS
    };
    let (right, right_style) = match &view.flash {
        Some(flash) => (vec![flash.as_str()], Style::default().fg(Color::Yellow)),
        None => (hints.to_vec(), Style::default().fg(Color::DarkGray)),
    };
    let least = right
        .iter()
        .map(|text| text.chars().count())
        .min()
        .unwrap_or(0);
    let mut items = items(transcript);
    // Items go, least useful first, until the shortest hint fits.
    while !items.is_empty() && width(&blocks) + width(&joined(&items)) + 2 + least > columns {
        let Some(at) = items
            .iter()
            .enumerate()
            .max_by_key(|(index, item)| (item.drop, *index))
            .map(|(index, _)| index)
        else {
            break;
        };
        items.remove(at);
    }
    let mut spans = blocks;
    spans.extend(joined(&items));
    let used = width(&spans);
    let room = columns.saturating_sub(used + 2);
    // A flash too long to sit beside the blocks takes the whole bar.
    if let Some(flash) = view
        .flash
        .as_ref()
        .filter(|flash| flash.chars().count() > room)
    {
        return Line::styled(flash.clone(), right_style);
    }
    if let Some(hint) = right.iter().find(|text| text.chars().count() <= room) {
        let pad = columns - used - hint.chars().count() - 1;
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled((*hint).to_owned(), right_style));
    }
    Line::from(spans)
}
