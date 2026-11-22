use crate::line::{Line, read};
use std::io::{BufReader, Cursor};

fn lines(input: &[u8], limit: usize, capacity: usize) -> Vec<(Line, Vec<u8>)> {
    let mut reader = BufReader::with_capacity(capacity, Cursor::new(input.to_vec()));
    let mut buffer = Vec::new();
    let mut read_lines = Vec::new();
    loop {
        let line = read(&mut reader, &mut buffer, limit).unwrap();
        if line == Line::End {
            return read_lines;
        }
        read_lines.push((line, buffer.clone()));
    }
}

#[test]
fn splits_on_newlines_and_drops_crlf_endings() {
    let read_lines = lines(b"one\r\ntwo\n\nlast", 16, 3);
    assert_eq!(
        read_lines,
        vec![
            (Line::Read, b"one".to_vec()),
            (Line::Read, b"two".to_vec()),
            (Line::Read, Vec::new()),
            (Line::Read, b"last".to_vec()),
        ]
    );
}

#[test]
fn a_long_line_is_dropped_whole_and_the_next_one_still_reads() {
    let mut input = vec![b'x'; 40];
    input.extend_from_slice(b"\nok\n");
    let read_lines = lines(&input, 8, 4);
    assert_eq!(
        read_lines,
        vec![(Line::TooLong, Vec::new()), (Line::Read, b"ok".to_vec())]
    );
}

#[test]
fn the_bound_is_exact_with_or_without_a_carriage_return() {
    assert_eq!(
        lines(b"12345678\n", 8, 64),
        vec![(Line::Read, b"12345678".to_vec())]
    );
    assert_eq!(
        lines(b"12345678\r\n", 8, 64),
        vec![(Line::Read, b"12345678".to_vec())]
    );
    assert_eq!(
        lines(b"123456789\n", 8, 64),
        vec![(Line::TooLong, Vec::new())]
    );
    assert_eq!(
        lines(b"123456789", 8, 64),
        vec![(Line::TooLong, Vec::new())]
    );
}

#[test]
fn a_dropped_line_is_never_held_beyond_the_bound() {
    let input = vec![b'x'; 10_000];
    let mut reader = BufReader::with_capacity(16, Cursor::new(input));
    let mut buffer = Vec::new();
    assert_eq!(read(&mut reader, &mut buffer, 32).unwrap(), Line::TooLong);
    assert!(buffer.capacity() <= 64, "kept {} bytes", buffer.capacity());
}
