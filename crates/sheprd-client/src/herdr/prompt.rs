//! The question a terminal agent waits on, read from its screen: Claude
//! Code's multiple-choice questions and permission prompts and Codex's
//! approvals all draw a numbered menu with a marker on the highlighted
//! choice. Pure: the caller reads the screen's text and moves the
//! highlight with keys.

use serde::{Deserialize, Serialize};

/// Lines above the menu kept as its question.
const MAX_CONTEXT: usize = 8;
/// Choices a menu may have; more is not a prompt.
const MAX_CHOICES: usize = 12;
/// Characters kept of one line.
const MAX_LINE: usize = 400;
/// What marks the highlighted choice: Claude Code's `❯`, Codex's `›`.
const MARKERS: [char; 3] = ['❯', '›', '>'];

/// A numbered menu waiting on the person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    /// What it asks, top to bottom: the question and what it is about,
    /// such as the command a permission prompt shows.
    pub question: Vec<String>,
    pub choices: Vec<Choice>,
    /// The highlighted choice's index in `choices`.
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// The number the menu shows.
    pub number: u32,
    pub label: String,
    /// What the menu says under it, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// It takes typed words: highlighted, what is typed goes in it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub text: bool,
}

/// One numbered line: where its number starts, the number, the label, and
/// whether the marker is on it.
struct Numbered<'a> {
    line: usize,
    column: usize,
    number: u32,
    label: &'a str,
    marked: bool,
}

/// The menu at the bottom of `screen`, when there is one: a run numbered
/// from 1 with exactly one choice marked. A numbered list in an answer has
/// no marker, so it is never taken for a menu.
pub fn read(screen: &str) -> Option<Prompt> {
    let lines: Vec<&str> = screen.lines().map(unboxed).collect();
    let numbered: Vec<Numbered> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| numbered(index, line))
        .collect();
    // The last run that counts up from 1.
    let start = numbered.iter().rposition(|entry| entry.number == 1)?;
    let mut run = vec![&numbered[start]];
    for entry in &numbered[start + 1..] {
        if entry.number != run.last()?.number + 1 || run.len() >= MAX_CHOICES {
            break;
        }
        run.push(entry);
    }
    let marked: Vec<usize> = run
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.marked.then_some(index))
        .collect();
    let [selected] = marked.as_slice() else {
        return None;
    };
    let choices = run
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let end = run.get(index + 1).map_or(lines.len(), |next| next.line);
            let detail = lines[entry.line + 1..end]
                .iter()
                .take_while(|line| indent(line) > entry.column && !rule(line))
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Choice {
                number: entry.number,
                label: clip(entry.label),
                text: entry.label.starts_with("Type something"),
                detail: (!detail.is_empty()).then(|| clip(&detail)),
            }
        })
        .collect();
    Some(Prompt {
        question: question(&lines[..run[0].line]),
        choices,
        selected: *selected,
    })
}

/// The keys that move the highlight from `prompt`'s choice to `target`'s
/// index and take it.
pub fn keys_to(prompt: &Prompt, target: usize) -> Vec<&'static str> {
    let (key, steps) = if target >= prompt.selected {
        ("down", target - prompt.selected)
    } else {
        ("up", prompt.selected - target)
    };
    let mut keys = vec![key; steps];
    keys.push("enter");
    keys
}

/// The lines just above the menu, back to a rule or a gap after text.
fn question(above: &[&str]) -> Vec<String> {
    let mut kept = Vec::new();
    for line in above.iter().rev() {
        let trimmed = line.trim();
        if rule(line) || kept.len() >= MAX_CONTEXT {
            break;
        }
        if trimmed.is_empty() {
            if kept.is_empty() {
                continue;
            }
            // A gap ends the question once it has a few lines; a framed
            // permission prompt's command sits between gaps of its own.
            if kept.len() >= 4 {
                break;
            }
            continue;
        }
        kept.push(clip(
            trimmed.trim_start_matches(['☐', '☒', '✔', '│']).trim(),
        ));
    }
    kept.reverse();
    kept.retain(|line| !line.is_empty());
    kept
}

fn numbered<'a>(index: usize, line: &'a str) -> Option<Numbered<'a>> {
    let trimmed = line.trim_start();
    let (marked, rest) = match trimmed.strip_prefix(MARKERS) {
        Some(rest) => (true, rest.trim_start()),
        None => (false, trimmed),
    };
    let digits = rest.find(|c: char| !c.is_ascii_digit())?;
    let number = rest[..digits].parse().ok()?;
    let label = rest[digits..].strip_prefix(". ")?.trim();
    if label.is_empty() {
        return None;
    }
    Some(Numbered {
        line: index,
        column: line.len() - rest.len(),
        number,
        label,
        marked,
    })
}

/// A line without the box a framed prompt draws around it.
fn unboxed(line: &str) -> &str {
    let line = line.trim_end();
    let line = line.strip_suffix('│').unwrap_or(line).trim_end();
    match line.trim_start().strip_prefix('│') {
        Some(rest) => rest,
        None => line,
    }
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A horizontal rule or a box's edge.
fn rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= 3
        && trimmed
            .chars()
            .all(|c| "─━═╭╮╰╯-".contains(c) || c.is_whitespace())
}

fn clip(text: &str) -> String {
    text.chars().take(MAX_LINE).collect()
}

#[cfg(test)]
mod tests;
