#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn messages_round_trip_through_any_split() {
    let messages: Vec<Vec<u8>> = vec![b"".to_vec(), b"a".to_vec(), vec![7; 300], b"end".to_vec()];
    let stream: Vec<u8> = messages
        .iter()
        .flat_map(|message| encode(message).unwrap())
        .collect();
    for chunk in 1..=stream.len() {
        let mut deframer = Deframer::default();
        let mut read = Vec::new();
        for piece in stream.chunks(chunk) {
            if let Some(message) = deframer.push(piece).unwrap() {
                read.push(message);
            }
            while let Some(message) = deframer.next().unwrap() {
                read.push(message);
            }
        }
        assert_eq!(read, messages, "chunks of {chunk}");
        assert_eq!(deframer.buffered(), 0);
    }
}

#[test]
fn the_prefix_is_big_endian() {
    assert_eq!(encode(&[9; 258]).unwrap()[..3], [1, 2, 9]);
    assert_eq!(encode(&[]).unwrap(), [0, 0]);
}

#[test]
fn too_long_a_message_is_refused() {
    assert!(encode(&vec![0; MAX_MESSAGE]).is_ok());
    assert_eq!(
        encode(&vec![0; MAX_MESSAGE + 1]),
        Err(Error::TooLong {
            length: MAX_MESSAGE + 1
        })
    );
}

#[test]
fn a_message_over_the_cap_is_refused_from_its_prefix() {
    let mut deframer = Deframer::new(16);
    assert_eq!(
        deframer.push(&[0, 17]),
        Err(Error::OverCap {
            cap: 16,
            length: 17
        })
    );
    let mut deframer = Deframer::new(16);
    assert_eq!(deframer.push(&[0, 16]), Ok(None));
    assert_eq!(deframer.wanted(), 16);
    assert_eq!(deframer.push(&[1; 16]), Ok(Some(vec![1; 16])));
}

#[test]
fn wanted_counts_down_to_a_whole_message() {
    let mut deframer = Deframer::default();
    assert_eq!(deframer.wanted(), 2);
    assert_eq!(deframer.push(&[0]), Ok(None));
    assert_eq!(deframer.wanted(), 1);
    assert_eq!(deframer.push(&[3, 1]), Ok(None));
    assert_eq!(deframer.wanted(), 2);
    assert_eq!(deframer.push(&[2, 3]), Ok(Some(vec![1, 2, 3])));
    assert_eq!(deframer.wanted(), 2);
}
