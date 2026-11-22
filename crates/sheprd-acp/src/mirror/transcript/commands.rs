//! What Claude Code records when the person runs one of its own commands in
//! the terminal: a slash command, its output, or a `!` shell line. Claude
//! writes each as a user message in its own markup, which a chat would show
//! as raw tags under the person's name. Only a message that is wholly one
//! of these envelopes counts: its known tags, each once, with nothing but
//! whitespace between them. Any other text keeps the message the person's.

use sheprd_chat_protocol::EntryKind;

use super::notices;

/// What a command record becomes in the chat.
#[derive(Debug, PartialEq)]
pub(super) enum Command {
    /// A notice in place of the record.
    Show(EntryKind),
    /// Nothing: output that says nothing, such as `(no content)`.
    Hide,
}

/// The tags of a slash command's record.
const SLASH: [&str; 3] = ["command-name", "command-message", "command-args"];
/// The tags of a slash command's output.
const OUTPUT: [&str; 2] = ["local-command-stdout", "local-command-stderr"];
/// The tag of a `!` shell line.
const SHELL: [&str; 1] = ["bash-input"];
/// The tags of a `!` shell line's output.
const SHELL_OUTPUT: [&str; 2] = ["bash-stdout", "bash-stderr"];

/// The command `content` records, or `None` for anything else.
pub(super) fn command(content: &str) -> Option<Command> {
    if let Some(tags) = envelope(content, &SLASH) {
        let name = tags.get("command-name")?.trim();
        if name.is_empty() {
            return None;
        }
        let args = tags.get("command-args").unwrap_or_default().trim();
        let line = if args.is_empty() {
            name.to_owned()
        } else {
            format!("{name} {args}")
        };
        return Some(Command::Show(notices::source(
            format!("Ran {}", notices::compact(&line)),
            content.trim(),
        )));
    }
    if let Some(tags) = envelope(content, &OUTPUT) {
        return Some(output_notice("Command output", &tags.joined()));
    }
    if let Some(tags) = envelope(content, &SHELL) {
        let line = tags.get("bash-input")?;
        return Some(Command::Show(notices::source(
            format!("Ran ! {}", notices::compact(line)),
            line,
        )));
    }
    if let Some(tags) = envelope(content, &SHELL_OUTPUT) {
        return Some(output_notice("Shell output", &tags.joined()));
    }
    None
}

fn output_notice(label: &str, output: &str) -> Command {
    let output = output.trim();
    if output.is_empty() || output == "(no content)" {
        return Command::Hide;
    }
    let first = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    Command::Show(notices::source(
        format!("{label} · {}", notices::compact(first)),
        output,
    ))
}

/// The tags of an envelope, in the order they came.
struct Tags<'a>(Vec<(&'static str, &'a str)>);

impl<'a> Tags<'a> {
    fn get(&self, tag: &str) -> Option<&'a str> {
        self.0
            .iter()
            .find(|(name, _)| *name == tag)
            .map(|(_, inner)| *inner)
    }

    /// Every tag's text that says something, one per line.
    fn joined(&self) -> String {
        self.0
            .iter()
            .map(|(_, inner)| inner.trim())
            .filter(|inner| !inner.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `content` read as nothing but tags from `allowed`, each at most once and
/// separated only by whitespace, or `None` when any other text is there.
fn envelope<'a>(content: &'a str, allowed: &[&'static str]) -> Option<Tags<'a>> {
    let mut rest = content.trim();
    let mut tags = Vec::new();
    while !rest.is_empty() {
        let (tag, after) = allowed.iter().find_map(|tag| {
            let after = rest
                .strip_prefix('<')?
                .strip_prefix(*tag)?
                .strip_prefix('>')?;
            Some((*tag, after))
        })?;
        if tags.iter().any(|(seen, _)| *seen == tag) {
            return None;
        }
        let close = format!("</{tag}>");
        let end = after.find(&close)?;
        tags.push((tag, &after[..end]));
        rest = after[end + close.len()..].trim_start();
    }
    (!tags.is_empty()).then_some(Tags(tags))
}
