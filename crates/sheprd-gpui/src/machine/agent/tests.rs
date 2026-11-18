#![allow(clippy::unwrap_used)]
use super::wire::*;
use super::*;
use std::io::{Cursor, Read, Write};

/// A stream that hands back `reply` and keeps what was written.
struct Peer {
    reply: Cursor<Vec<u8>>,
    sent: Vec<u8>,
}

impl Read for Peer {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reply.read(buffer)
    }
}

impl Write for Peer {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.sent.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn framed(body: &[u8]) -> Vec<u8> {
    let mut framed = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
    framed.extend_from_slice(body);
    framed
}

#[test]
fn a_request_is_framed_json_and_its_reply_is_read_whole() {
    let mut peer = Peer {
        reply: Cursor::new(framed(br#"{"ok":true}"#)),
        sent: Vec::new(),
    };
    let request = Request::Type { text: "hi".into() };
    let reply = exchange(&mut peer, &request).unwrap();
    assert_eq!(reply, br#"{"ok":true}"#);
    let length = u32::from_be_bytes(peer.sent[..4].try_into().unwrap()) as usize;
    assert_eq!(length, peer.sent.len() - 4);
    let sent: serde_json::Value = serde_json::from_slice(&peer.sent[4..]).unwrap();
    assert_eq!(sent, serde_json::json!({"op": "type", "text": "hi"}));
}

#[test]
fn requests_are_spelled_as_the_agent_reads_them() {
    use sheprd_control_protocol::protocol::tabs::{MachineButton, MachineModifier};
    let cases = [
        (Request::Hello, serde_json::json!({"op": "hello"})),
        (
            Request::Exec {
                argv: vec!["ls".into()],
                cwd: None,
                timeout_ms: 1000,
            },
            serde_json::json!({"op": "exec", "argv": ["ls"], "timeout_ms": 1000}),
        ),
        (
            Request::Click {
                x: 1,
                y: 2,
                button: MachineButton::Right,
                count: 2,
            },
            serde_json::json!({"op": "click", "x": 1, "y": 2, "button": "right", "count": 2}),
        ),
        (
            Request::Key {
                key: "q".into(),
                modifiers: vec![MachineModifier::Cmd, MachineModifier::Option],
            },
            serde_json::json!({"op": "key", "key": "q", "modifiers": ["cmd", "option"]}),
        ),
    ];
    for (request, wire) in cases {
        assert_eq!(serde_json::to_value(&request).unwrap(), wire);
    }
}

#[test]
fn a_reply_past_the_limit_is_refused_before_it_is_read() {
    let prefix = u32::try_from(MAX_REPLY + 1).unwrap().to_be_bytes();
    let error = read_reply(Cursor::new(prefix.to_vec())).unwrap_err();
    assert!(matches!(error, Error::TooLarge(_)), "{error:?}");
}

#[test]
fn a_cut_reply_is_lost_not_parsed() {
    let mut cut = framed(br#"{"ok":true}"#);
    cut.truncate(8);
    let error = read_reply(Cursor::new(cut)).unwrap_err();
    assert!(matches!(error, Error::Io(_)), "{error:?}");
}

#[test]
fn a_refusal_says_why() {
    let body = br#"{"ok":false,"error":"accessibility","message":"allow it"}"#;
    let error = parse::<Done>(body).unwrap_err();
    assert!(
        matches!(
            &error,
            Error::Refused {
                refusal: Refusal::Accessibility,
                message,
            } if message == "allow it"
        ),
        "{error:?}"
    );
}

#[test]
fn an_unknown_refusal_is_unreadable() {
    let error = parse::<Done>(br#"{"ok":false,"error":"tired"}"#).unwrap_err();
    assert!(matches!(error, Error::Reply(_)), "{error:?}");
}

#[test]
fn a_command_that_a_signal_ended_has_no_status() {
    let body = br#"{"ok":true,"stdout":"","stderr":"","truncated":false,"timed_out":true}"#;
    let exec: Exec = parse(body).unwrap();
    assert_eq!(exec.status, None);
    assert!(exec.timed_out);
}

#[test]
fn hello_reads_the_screen_and_permissions() {
    let body = br#"{"ok":true,"version":1,"width":2560,"height":1600,"scale":2,
        "accessibility":true,"screen_recording":false}"#;
    let hello: Hello = parse(body).unwrap();
    assert_eq!((hello.width, hello.height), (2560, 1600));
    assert!(hello.accessibility && !hello.screen_recording);
}

#[test]
fn output_keeps_its_end_and_whole_characters() {
    assert_eq!(tail("short".into(), 10), ("short".into(), false));
    assert_eq!(tail("abcdef".into(), 3), ("def".into(), true));
    // "é" is two bytes: a cut through it moves past it.
    assert_eq!(tail("aéb".into(), 2), ("b".into(), true));
}

#[test]
fn a_png_header_gives_its_size() {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend_from_slice(&2528_u32.to_be_bytes());
    png.extend_from_slice(&1706_u32.to_be_bytes());
    png.extend_from_slice(b"rest of the image");
    assert_eq!(png_size(&png), Some((2528, 1706)));
}

#[test]
fn bytes_that_are_not_a_png_have_no_size() {
    // The guest is not trusted with what lands at the caller's path.
    assert_eq!(png_size(b"#!/bin/sh\nrm -rf ~\n and some more bytes"), None);
    assert_eq!(png_size(b"\x89PNG\r\n\x1a\n"), None);
    let mut wrong_chunk = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIDAT".to_vec();
    wrong_chunk.extend_from_slice(&[0; 8]);
    assert_eq!(png_size(&wrong_chunk), None);
}
