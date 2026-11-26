//! An agent's Markdown as the plain text a notification shows: no emphasis
//! or code marks, links as their text, and no `:big[…]` size marks
//! (`docs/rich-chat.md`, Text size and effects), as a lock screen draws
//! none of them and has few characters to spare.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// `markdown` as plain text on one line. Whole directive lines are
/// [`super::directives::without_directives`]'s business.
pub(super) fn plain(markdown: &str) -> String {
    Sized::new(markdown).text
}

/// [`plain`], led by the sentence the agent set larger: a `:big[…]`,
/// `:huge[…]` or `:giant[…]` span marks the outcome, which a short excerpt
/// should not cut off. The rest follows in its order.
pub(super) fn verdict_first(markdown: &str) -> String {
    let sized = Sized::new(markdown);
    let text = &sized.text;
    let Some(span) = sized.larger.clone() else {
        return sized.text;
    };
    let sentence = sized.sentence(span);
    if sentence.start == 0 {
        return sized.text;
    }
    let parts = [
        &text[sentence.clone()],
        &text[..sentence.start],
        &text[sentence.end..],
    ];
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The plain line of a reply, with where its first larger span and its
/// blocks' ends (paragraphs, headings, list items) ended up.
struct Sized {
    text: String,
    larger: Option<Range<usize>>,
    /// Offsets in `text` where a block ended, which end a sentence too.
    blocks: Vec<usize>,
}

impl Sized {
    fn new(markdown: &str) -> Self {
        let mut line = Line::default();
        for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
            match event {
                Event::Text(part) => line.push(&part, line.in_code),
                // Code is quoted, never marked up: `:big[x]` in it is text.
                Event::Code(part) => line.push(&part, true),
                Event::SoftBreak | Event::HardBreak => line.push(" ", false),
                Event::Start(Tag::CodeBlock(_)) => line.in_code = true,
                Event::End(
                    end @ (TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::Item
                    | TagEnd::CodeBlock
                    | TagEnd::BlockQuote(_)),
                ) => {
                    if end == TagEnd::CodeBlock {
                        line.in_code = false;
                    }
                    line.blocks.push(line.text.len());
                    line.push(" ", false);
                }
                _ => {}
            }
        }
        line.sized()
    }

    /// The sentence of `text` holding `span`: from just after the last
    /// sentence or block end before it to the first one at or after its
    /// end.
    fn sentence(&self, span: Range<usize>) -> Range<usize> {
        let text = &self.text;
        let before = &text[..span.start];
        let start = before
            .char_indices()
            .rev()
            .find(|(at, ch)| ends_sentence(*ch) && before[at + ch.len_utf8()..].starts_with(' '))
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        let start = self
            .blocks
            .iter()
            .copied()
            .filter(|block| *block <= span.start)
            .fold(start, usize::max);
        let block_end = self
            .blocks
            .iter()
            .copied()
            .find(|block| *block >= span.end)
            .unwrap_or(text.len());
        if text[..span.end]
            .chars()
            .next_back()
            .is_some_and(ends_sentence)
        {
            return start..span.end;
        }
        let after = &text[span.end..];
        let end = after
            .char_indices()
            .find(|(at, ch)| {
                let next = &after[at + ch.len_utf8()..];
                ends_sentence(*ch) && (next.is_empty() || next.starts_with(' '))
            })
            .map_or(text.len(), |(at, ch)| span.end + at + ch.len_utf8());
        start..end.min(block_end)
    }
}

/// The reply's text as it is read, before size marks come out: whitespace
/// already collapsed, and which bytes are code.
#[derive(Default)]
struct Line {
    text: String,
    code: Vec<Range<usize>>,
    blocks: Vec<usize>,
    in_code: bool,
}

impl Line {
    /// Appends `part`, each run of whitespace as one space, none leading.
    fn push(&mut self, part: &str, code: bool) {
        let start = self.text.len();
        for ch in part.chars() {
            if ch.is_whitespace() {
                if !self.text.is_empty() && !self.text.ends_with(' ') {
                    self.text.push(' ');
                }
            } else {
                self.text.push(ch);
            }
        }
        if code && self.text.len() > start {
            self.code.push(start..self.text.len());
        }
    }

    fn is_code(&self, at: usize) -> bool {
        self.code.iter().any(|range| range.contains(&at))
    }

    /// The line with each size mark outside code reduced to what it holds,
    /// and where the first outermost span larger than the prose ended up.
    /// A mark opens only at the start or after a character that is not
    /// part of a word, so `http://x:big[` stays text; brackets inside a
    /// span pair up, and a span never closed loses only its opening.
    fn sized(self) -> Sized {
        let text = self.text.trim_end();
        let mut out = String::with_capacity(text.len());
        let mut blocks = Vec::with_capacity(self.blocks.len());
        let mut pending = self.blocks.iter().copied().peekable();
        // Each open span's count of its own brackets still open.
        let mut open: Vec<usize> = Vec::new();
        let mut larger: Option<Range<usize>> = None;
        // Where the outermost span open now started, when it is larger.
        let mut opening: Option<usize> = None;
        let mut at = 0;
        loop {
            while pending.next_if(|block| *block <= at).is_some() {
                blocks.push(out.len());
            }
            let Some(ch) = text[at..].chars().next() else {
                break;
            };
            let rest = &text[at + ch.len_utf8()..];
            let code = self.is_code(at);
            match ch {
                ':' if !code && open.len() < MAX_OPEN && starts_mark(&out) => {
                    if let Some(name) = SIZES
                        .into_iter()
                        .find(|name| rest.strip_prefix(name).is_some_and(|r| r.starts_with('[')))
                    {
                        if open.is_empty() && name != "small" && larger.is_none() {
                            opening = Some(out.len());
                        }
                        open.push(1);
                        at += 1 + name.len() + 1;
                        continue;
                    }
                    out.push(ch);
                }
                '[' if !code && !open.is_empty() => {
                    if let Some(depth) = open.last_mut() {
                        *depth += 1;
                    }
                    out.push(ch);
                }
                ']' if !code && !open.is_empty() => {
                    let closed = open.last_mut().is_some_and(|depth| {
                        *depth -= 1;
                        *depth == 0
                    });
                    if closed {
                        open.pop();
                        if open.is_empty()
                            && let Some(start) = opening.take()
                            && start < out.len()
                        {
                            larger = Some(start..out.len());
                        }
                    } else {
                        out.push(ch);
                    }
                }
                _ => out.push(ch),
            }
            at += ch.len_utf8();
        }
        Sized {
            text: out,
            larger,
            blocks,
        }
    }
}

/// Whether `ch` ends a sentence when a space or the end follows it.
fn ends_sentence(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?')
}

/// The steps a size mark may name, as the chat draws them; only `small`
/// is no larger than the prose.
const SIZES: [&str; 4] = ["small", "big", "huge", "giant"];

/// Spans open at once; deeper openings stay text, as in the chat.
const MAX_OPEN: usize = 8;

/// Whether a mark may open after `before`.
fn starts_mark(before: &str) -> bool {
    before
        .chars()
        .next_back()
        .is_none_or(|ch| !ch.is_alphanumeric() && ch != '/')
}
