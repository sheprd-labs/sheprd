//! What a terminal agent last said, read off its visible screen. An agent's
//! own interface sits below its words: an input box, a prompt, a status or
//! spinner line, and a footer of hints. Those are skipped from the bottom
//! up, and the paragraph above them is the preview. An agent waiting on a
//! numbered menu (a question, a permission, an approval) shows what it
//! asks instead, never its choices. The rules are a best guess at Claude
//! Code, Codex, and OpenCode's screens, not a parser of any.

use crate::ansi;
use sheprd_client::herdr::prompt;

/// The longest preview, in characters; a phone's row shows a few lines.
pub(super) const MAX_CHARS: usize = 280;
/// Lines looked at from the bottom of the screen.
const MAX_LINES: usize = 200;
/// Lines of one paragraph kept, its last ones.
const MAX_PARAGRAPH: usize = 12;
/// Leading spaces that make a line a right-aligned hint, not text.
const RIGHT_ALIGNED: usize = 30;
/// Spaces inside a line that split it into a status bar's left and right.
const STATUS_GAP: &str = "          ";
/// Glyphs an agent starts a spinner or status line with.
const SPINNERS: &[char] = &['✻', '✽', '✶', '✳', '✢', '✺', '✷', '✸', '✹'];
/// Glyphs that start a message, a tool call, or a list item.
const MARKERS: &[&str] = &["⏺", "●", "•", "※", "⎿", "↳", "- ", "* "];
/// Glyphs an input box draws its sides with.
const BOX_SIDES: &[char] = &['│', '┃', '▌', '▐', '╭', '╰', '╮', '╯'];
/// Text only an agent's own hints and footers say.
const HINTS: &[&str] = &[
    "⏵⏵",
    "? for shortcuts",
    "esc to interrupt",
    "shift+tab",
    "ctrl+c to",
];
/// Hints an agent ends its text with, cut from a preview.
const TRAILING_HINTS: &[&str] = &["(disable recaps in /config)"];

/// The last paragraph above the agent's own interface on `screen` (text
/// with SGR colors, as `pane.read` gives it), one item to a line, or the
/// question of a menu it waits on; `None` when the screen shows nothing
/// else.
pub(crate) fn preview(screen: &str) -> Option<String> {
    let screen = ansi::parse(ansi::tail(screen));
    let skip = screen.lines.len().saturating_sub(MAX_LINES);
    let lines: Vec<String> = screen.lines[skip..]
        .iter()
        .map(|runs| {
            runs.iter()
                .filter(|(style, _)| !style.hidden)
                .map(|(_, text)| text.as_str())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    if let Some(menu) = prompt::read(&lines.join("\n")) {
        let text = menu.question.join("\n");
        return (!text.is_empty()).then(|| bounded(&text));
    }
    let mut paragraph = Vec::new();
    for line in lines.iter().rev() {
        match (kind(line), paragraph.is_empty()) {
            (Kind::Text, _) if paragraph.len() < MAX_PARAGRAPH => paragraph.push(line.trim()),
            (Kind::Text, _) => break,
            (Kind::Blank | Kind::Chrome, true) => {}
            (Kind::Blank | Kind::Chrome, false) => break,
        }
    }
    paragraph.reverse();
    let text = join(&paragraph);
    (!text.is_empty()).then(|| bounded(&text))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Blank,
    /// The agent's interface: a rule, a box, a prompt, a status, a hint.
    Chrome,
    Text,
}

fn kind(line: &str) -> Kind {
    let text = line.trim();
    let Some(first) = text.chars().next() else {
        return Kind::Blank;
    };
    let indent = line.len() - line.trim_start().len();
    let chrome = rule(text)
        || BOX_SIDES.contains(&first)
        || prompt(text)
        || SPINNERS.contains(&first)
        || (matches!(first, '·' | '*') && text.contains('…'))
        || HINTS.iter().any(|hint| text.contains(hint))
        || text.matches(" · ").count() >= 2
        || indent >= RIGHT_ALIGNED
        || text.contains(STATUS_GAP);
    if chrome { Kind::Chrome } else { Kind::Text }
}

/// A line drawn mostly with box-drawing or block characters.
fn rule(text: &str) -> bool {
    let drawn = |c: char| matches!(c, '\u{2500}'..='\u{259f}');
    let (total, ruled) = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .fold((0, 0), |(total, ruled), c| {
            (total + 1, ruled + usize::from(drawn(c)))
        });
    total >= 8 && ruled * 5 >= total * 4
}

/// An input line: Claude Code's `❯`, Codex's `›`, or a bare `>`.
fn prompt(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some('❯' | '›' | '>')) && chars.next().is_none_or(char::is_whitespace)
}

/// The paragraph's lines as items, one to a line: a line that starts with a
/// marker begins one, and the lines wrapped under it join it.
fn join(lines: &[&str]) -> String {
    let mut items: Vec<String> = Vec::new();
    for line in lines {
        let unmarked = MARKERS
            .iter()
            .find_map(|marker| line.strip_prefix(marker))
            .map(str::trim_start);
        match (unmarked, items.last_mut()) {
            (None, Some(item)) => {
                item.push(' ');
                item.push_str(line);
            }
            (unmarked, _) => items.push(unmarked.unwrap_or(line).to_owned()),
        }
    }
    for item in &mut items {
        for hint in TRAILING_HINTS {
            if let Some(kept) = item.strip_suffix(hint) {
                let kept = kept.trim_end().len();
                item.truncate(kept);
            }
        }
    }
    items.retain(|item| !item.is_empty());
    items.join("\n")
}

/// `text` cut to [`MAX_CHARS`], ending in an ellipsis when cut.
fn bounded(text: &str) -> String {
    match text.char_indices().nth(MAX_CHARS - 1) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
