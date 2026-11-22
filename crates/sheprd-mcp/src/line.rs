//! Reading one newline-terminated message without buffering more than the
//! bound: the rest of a longer line is read and dropped as it arrives.

use std::io::{self, BufRead};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// The input ended with no partial line left.
    End,
    /// A line, its terminator and any `\r` before it removed, is in the buffer.
    Read,
    /// The line was longer than the bound and was dropped up to its end.
    TooLong,
}

/// Reads the next line into `buffer`, which it clears first. A final line
/// without a newline still counts.
pub(crate) fn read(
    input: &mut impl BufRead,
    buffer: &mut Vec<u8>,
    limit: usize,
) -> io::Result<Line> {
    buffer.clear();
    let mut too_long = false;
    let mut any = false;
    loop {
        let available = match input.fill_buf() {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if available.is_empty() {
            return Ok(match (any, too_long) {
                (false, _) => Line::End,
                (true, true) => Line::TooLong,
                (true, false) => finish(buffer, limit),
            });
        }
        any = true;
        let (chunk, done) = match available.iter().position(|&byte| byte == b'\n') {
            Some(end) => (&available[..end], true),
            None => (available, false),
        };
        if !too_long {
            // The `\r` of a CRLF ending may still arrive within the bound.
            if buffer.len() + chunk.len() > limit + 1 {
                too_long = true;
                buffer.clear();
            } else {
                buffer.extend_from_slice(chunk);
            }
        }
        let used = chunk.len() + usize::from(done);
        input.consume(used);
        if done {
            return Ok(if too_long {
                Line::TooLong
            } else {
                finish(buffer, limit)
            });
        }
    }
}

/// Drops a CRLF ending's `\r`, then holds the line to the bound `read`
/// stretched by one byte for it.
fn finish(buffer: &mut Vec<u8>, limit: usize) -> Line {
    if buffer.last() == Some(&b'\r') {
        buffer.pop();
    }
    if buffer.len() > limit {
        buffer.clear();
        return Line::TooLong;
    }
    Line::Read
}
