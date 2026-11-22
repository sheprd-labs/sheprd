//! The effect directives an agent's reply may hold (`docs/rich-chat.md`,
//! Text size and effects), as far as a push cares: `::wizz` asks for the
//! person's attention, and no directive belongs in a notification's text.

/// Whether `line` is a directive: `::name`, `:::name`, or the `:::` that
/// ends a bubble, indented by three spaces at most, as a line indented
/// further is code.
fn directive(line: &str) -> Option<&str> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 || line.starts_with('\t') {
        return None;
    }
    let name = line.trim().strip_prefix("::")?;
    let name = name.strip_prefix(':').unwrap_or(name);
    name.chars().all(|c| c.is_ascii_lowercase()).then_some(name)
}

/// A code fence: its marker, and how long a run of it opened the fence.
#[derive(Clone, Copy)]
struct Fence {
    marker: char,
    length: usize,
}

impl Fence {
    /// The fence `line` opens: three or more backticks or tildes, indented
    /// by three spaces at most.
    fn open(line: &str) -> Option<Self> {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 {
            return None;
        }
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
        let length = trimmed.chars().take_while(|c| *c == marker).count();
        (length >= 3).then_some(Self { marker, length })
    }

    /// Whether `line` closes the fence: a run of its marker at least as
    /// long, and nothing after it.
    fn closes(self, line: &str) -> bool {
        let trimmed = line.trim_start_matches(' ');
        let run = trimmed.chars().take_while(|c| *c == self.marker).count();
        run >= self.length && trimmed[run * self.marker.len_utf8()..].trim().is_empty()
    }
}

/// The lines of `text`, each with whether it is prose: outside a code
/// fence, where a directive counts.
fn prose(text: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut fence: Option<Fence> = None;
    text.lines().map(move |line| match fence {
        Some(open) => {
            if open.closes(line) {
                fence = None;
            }
            (line, false)
        }
        None => {
            fence = Fence::open(line);
            (line, fence.is_none())
        }
    })
}

/// Whether the agent's reply asks for the person's attention: a `::wizz`
/// line outside code.
pub(super) fn wizzes(text: &str) -> bool {
    // `:::wizz` would open a bubble, not ask.
    prose(text).any(|(line, prose)| prose && line.trim() == "::wizz" && directive(line).is_some())
}

/// `text` without its directive lines, which only mean something drawn.
pub(super) fn without_directives(text: &str) -> String {
    prose(text)
        .filter(|(line, prose)| !(*prose && directive(line).is_some()))
        .map(|(line, _)| line)
        .collect::<Vec<_>>()
        .join("\n")
}
