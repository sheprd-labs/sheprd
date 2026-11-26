//! Bounded tmux control-mode reader. Captured screen text inside a reply
//! is always data, including lines that resemble asynchronous notifications.
use super::{Error, Result};
use std::{
    collections::VecDeque,
    io::{BufRead, Read},
    sync::mpsc::SyncSender,
};

pub(super) enum Message {
    Bytes(Vec<u8>),
    Resize(sheprd_backend::Size),
    Exit(sheprd_backend::Exit),
    Failed(Error),
}

fn decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut rest = data;
    while let Some((&byte, tail)) = rest.split_first() {
        if let [
            b'\\',
            a @ b'0'..=b'3',
            b @ b'0'..=b'7',
            c @ b'0'..=b'7',
            next @ ..,
        ] = rest
        {
            out.push((a - b'0') * 64 + (b - b'0') * 8 + (c - b'0'));
            rest = next;
        } else {
            out.push(byte);
            rest = tail;
        }
    }
    out
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Reply {
    Ignore,
    Dimensions,
    History,
    Saved,
    Capture,
    Cursor,
}

/// Captures use `capture-pane -J` so wrapped rows stay one logical line.
/// Because that joins rows, the reply line count is at most the physical row
/// count rather than equal to it. Each capture is immediately followed by a
/// per-attach random sentinel reply so pane text matching `%end <tag>` can
/// never close the capture block early.
struct CaptureBlock {
    tag: Vec<u8>,
    kind: Reply,
    max_lines: usize,
    emitted: usize,
    pending: PendingEnd,
}

enum PendingEnd {
    None,
    End {
        failed: bool,
        line: Vec<u8>,
    },
    Begin {
        failed: bool,
        end_line: Vec<u8>,
        begin_line: Vec<u8>,
        sentinel_tag: Vec<u8>,
    },
}

enum CaptureStep {
    Continue,
    Finished(Vec<u8>),
    Closed,
}

impl CaptureBlock {
    fn new(tag: Vec<u8>, kind: Reply, max_lines: usize) -> Self {
        Self {
            tag,
            kind,
            max_lines,
            emitted: 0,
            pending: PendingEnd::None,
        }
    }

    fn classify(&mut self, line: &[u8]) -> Option<PendingEnd> {
        if line.strip_prefix(b"%end ") == Some(self.tag.as_slice()) {
            Some(PendingEnd::End {
                failed: false,
                line: line.to_vec(),
            })
        } else if line.strip_prefix(b"%error ") == Some(self.tag.as_slice()) {
            Some(PendingEnd::End {
                failed: true,
                line: line.to_vec(),
            })
        } else {
            None
        }
    }

    fn emit(
        &mut self,
        line: &[u8],
        history: usize,
        captured_row: &mut bool,
        send: &SyncSender<Message>,
    ) -> Result<bool> {
        self.emitted += 1;
        if self.emitted > self.max_lines {
            return Err(Error::Invalid);
        }
        if self.kind == Reply::History && history == 0 {
            return Ok(true);
        }
        let mut bytes = Vec::with_capacity(line.len() + 2);
        if *captured_row {
            bytes.extend_from_slice(b"\r\n");
        }
        *captured_row = true;
        bytes.extend_from_slice(line);
        Ok(send.send(Message::Bytes(bytes)).is_ok())
    }

    fn push(
        &mut self,
        line: &[u8],
        sentinel: &[u8],
        history: usize,
        captured_row: &mut bool,
        send: &SyncSender<Message>,
    ) -> Result<CaptureStep> {
        match std::mem::replace(&mut self.pending, PendingEnd::None) {
            PendingEnd::None => {
                if let Some(pending) = self.classify(line) {
                    self.pending = pending;
                } else if !self.emit(line, history, captured_row, send)? {
                    return Ok(CaptureStep::Closed);
                }
            }
            PendingEnd::End {
                failed,
                line: end_line,
            } => {
                if let Some(next_tag) = line.strip_prefix(b"%begin ")
                    && next_tag.ends_with(b" 1")
                {
                    self.pending = PendingEnd::Begin {
                        failed,
                        end_line,
                        begin_line: line.to_vec(),
                        sentinel_tag: next_tag.to_vec(),
                    };
                } else {
                    if !self.emit(&end_line, history, captured_row, send)? {
                        return Ok(CaptureStep::Closed);
                    }
                    if let Some(pending) = self.classify(line) {
                        self.pending = pending;
                    } else if !self.emit(line, history, captured_row, send)? {
                        return Ok(CaptureStep::Closed);
                    }
                }
            }
            PendingEnd::Begin {
                failed,
                end_line,
                begin_line,
                sentinel_tag,
            } => {
                if line == sentinel {
                    if failed {
                        return Err(Error::Invalid);
                    }
                    return Ok(CaptureStep::Finished(sentinel_tag));
                }
                if !self.emit(&end_line, history, captured_row, send)?
                    || !self.emit(&begin_line, history, captured_row, send)?
                {
                    return Ok(CaptureStep::Closed);
                }
                if let Some(pending) = self.classify(line) {
                    self.pending = pending;
                } else if !self.emit(line, history, captured_row, send)? {
                    return Ok(CaptureStep::Closed);
                }
            }
        }
        Ok(CaptureStep::Continue)
    }
}

pub(super) fn read(
    mut stream: impl BufRead,
    pane: &str,
    history_limit: usize,
    sentinel: &str,
    send: &SyncSender<Message>,
    mut ready: Option<SyncSender<()>>,
) -> Result<()> {
    let send_bytes = |bytes| send.send(Message::Bytes(bytes)).is_ok();
    let prefix = format!("%output {pane} ");
    let mut expected = VecDeque::from([Reply::Dimensions]);
    let mut block: Option<(Vec<u8>, Reply)> = None;
    let mut capture: Option<CaptureBlock> = None;
    let mut captured_row = false;
    let mut history = 0;
    let mut height = 0;
    let mut alternate_on = false;
    let mut alternate = None;
    let mut seeded = false;
    const LIMIT: u64 = 4 << 20;
    let mut line = Vec::new();
    loop {
        line.clear();
        if stream.by_ref().take(LIMIT).read_until(b'\n', &mut line)? == 0 {
            return Err(Error::Invalid);
        }
        if line.pop() != Some(b'\n') {
            return Err(Error::Invalid);
        }
        if let Some(active) = capture.as_mut() {
            match active.push(&line, sentinel.as_bytes(), history, &mut captured_row, send)? {
                CaptureStep::Continue => {}
                CaptureStep::Closed => return Ok(()),
                CaptureStep::Finished(sentinel_tag) => {
                    let kind = active.kind;
                    capture = None;
                    if kind == Reply::Saved
                        && let Some(cursor) = alternate.take()
                    {
                        if !send_bytes(cursor) {
                            return Ok(());
                        }
                        captured_row = false;
                    }
                    block = Some((sentinel_tag, Reply::Ignore));
                }
            }
            continue;
        }
        if let Some((tag, kind)) = &block {
            if line.strip_prefix(b"%error ") == Some(tag.as_slice()) {
                return Err(Error::Invalid);
            }
            if line.strip_prefix(b"%end ") == Some(tag.as_slice()) {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(());
                }
                if *kind == Reply::Cursor {
                    seeded = true;
                }
                block = None;
            } else {
                match kind {
                    Reply::Dimensions => {
                        let facts = std::str::from_utf8(&line).map_err(|_| Error::Invalid)?;
                        let mut fields = facts.split_ascii_whitespace();
                        let mut parse = || {
                            fields
                                .next()
                                .and_then(|word| word.parse::<usize>().ok())
                                .ok_or(Error::Invalid)
                        };
                        history = parse()?.min(history_limit);
                        let rows = parse()?;
                        let cols = parse()?;
                        let size = sheprd_backend::Size::new(
                            cols.try_into().map_err(|_| Error::Invalid)?,
                            rows.try_into().map_err(|_| Error::Invalid)?,
                        )?;
                        height = rows;
                        if send.send(Message::Resize(size)).is_err() {
                            return Ok(());
                        }
                        if fields.next() == Some("1") {
                            let mut coordinate = || {
                                fields
                                    .next()
                                    .and_then(|s| s.parse::<u16>().ok())
                                    .map(|n| u32::from(n) + 1)
                                    .ok_or(Error::Invalid)
                            };
                            let (x, y) = (coordinate()?, coordinate()?);
                            alternate_on = true;
                            alternate =
                                Some(format!("\x1b[0m\x1b[{y};{x}H\x1b[?1049h\x1b[H").into_bytes());
                            expected.extend([
                                Reply::Ignore,
                                Reply::History,
                                Reply::Saved,
                                Reply::Capture,
                                Reply::Cursor,
                            ]);
                        } else {
                            expected.extend([Reply::Ignore, Reply::Capture, Reply::Cursor]);
                        }
                    }
                    Reply::History | Reply::Saved | Reply::Capture => return Err(Error::Invalid),
                    Reply::Cursor => {
                        let bytes = cursor(&line)?;
                        if !send_bytes(bytes) {
                            return Ok(());
                        }
                    }
                    Reply::Ignore => {}
                }
            }
        } else if let Some(tag) = line.strip_prefix(b"%begin ") {
            let kind = if tag.ends_with(b" 1") {
                expected.pop_front().unwrap_or(Reply::Ignore)
            } else {
                Reply::Ignore
            };
            match kind {
                Reply::History => {
                    capture = Some(CaptureBlock::new(tag.to_vec(), kind, history.max(1)));
                }
                Reply::Saved => {
                    capture = Some(CaptureBlock::new(tag.to_vec(), kind, height));
                }
                Reply::Capture => {
                    let max_lines = if alternate_on {
                        height
                    } else {
                        history.saturating_add(height)
                    };
                    capture = Some(CaptureBlock::new(tag.to_vec(), kind, max_lines));
                }
                _ => {
                    block = Some((tag.to_vec(), kind));
                }
            }
        } else if let Some(layout) = line.strip_prefix(b"%layout-change ")
            && let Some(size) = layout_size(layout)
        {
            if send.send(Message::Resize(size)).is_err() {
                return Ok(());
            }
        } else if let Some(event) = line.strip_prefix(b"%subscription-changed sheprd-exit ") {
            if let Some(status) = event.split(|c| *c == b':').nth(1)
                && let Ok(value) = std::str::from_utf8(status)
                && let Some(code) = value.trim_start().strip_prefix("1/")
            {
                let _ = send.send(Message::Exit(sheprd_backend::Exit {
                    code: code.trim().parse().ok(),
                }));
                return Ok(());
            }
        } else if seeded && let Some(data) = line.strip_prefix(prefix.as_bytes()) {
            if !send_bytes(decode(data)) {
                return Ok(());
            }
        } else if line == b"%exit" || line.starts_with(b"%exit ") {
            let _ = send.send(Message::Exit(sheprd_backend::Exit::default()));
            return Ok(());
        }
    }
}

/// The dedicated session has a single window and pane. tmux sends a layout
/// after another attached client resizes it, including to observers.
fn layout_size(layout: &[u8]) -> Option<sheprd_backend::Size> {
    let layout = std::str::from_utf8(layout)
        .ok()?
        .split_ascii_whitespace()
        .nth(1)?;
    let (cols, rows) = layout.split(',').nth(1)?.split_once('x')?;
    sheprd_backend::Size::new(cols.parse().ok()?, rows.parse().ok()?).ok()
}

fn cursor(line: &[u8]) -> Result<Vec<u8>> {
    let mut facts = std::str::from_utf8(line)
        .map_err(|_| Error::Invalid)?
        .split_ascii_whitespace();
    let mut coord = || {
        facts
            .next()
            .and_then(|n| n.parse::<u16>().ok())
            .map(|n| u32::from(n) + 1)
            .ok_or(Error::Invalid)
    };
    let (x, y) = (coord()?, coord()?);
    let mut bytes = format!("\x1b[0m\x1b[{y};{x}H").into_bytes();
    for enabled in [
        b"\x1b[?1h".as_slice(),
        b"\x1b[?2004h",
        b"\x1b[?1000h",
        b"\x1b[?1002h",
        b"\x1b[?1003h",
        b"\x1b[?1006h",
        b"\x1b[?1005h",
    ] {
        if facts.next() == Some("1") {
            bytes.extend_from_slice(enabled);
        }
    }
    if facts.next() == Some("0") {
        bytes.extend_from_slice(b"\x1b[?25l");
    }
    Ok(bytes)
}
