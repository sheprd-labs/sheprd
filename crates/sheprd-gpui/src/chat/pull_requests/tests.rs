#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use sheprd_chat_protocol::{Cursor, PullRequestAction, Rejection, Transcript};
use sheprd_client::private_endpoint::Listener;
use std::{ffi::OsString, thread};

const URL: &str = "https://github.com/o/r/pull/7";

/// A private address for one test's chat.
fn private_address(dir: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!(
            r"\\.\pipe\sheprd-chat-pr-test-{}",
            uuid::Uuid::new_v4().simple()
        ))
    } else {
        dir.join("chat.sock")
    }
}

/// A chat that greets one client with `transcript`, then answers its one
/// command with `answer` after a change of no interest; what it was sent.
fn chat(
    address: &Path,
    transcript: Transcript,
    answer: Option<Rejection>,
) -> thread::JoinHandle<Vec<ClientMessage>> {
    let listener = Listener::bind(address).unwrap();
    thread::spawn(move || {
        let (reader, writer, _closer) = listener.accept().unwrap().split().unwrap();
        let (mut reader, mut writer) = (BufReader::new(reader), BufWriter::new(writer));
        let mut heard = vec![
            read_frame::<_, ClientMessage>(&mut reader)
                .unwrap()
                .unwrap(),
        ];
        let cursor = Cursor::new_epoch();
        let welcome = ServerMessage::Welcome {
            agent: "Claude".into(),
            pane: None,
            version: VERSION,
        };
        let snapshot = ServerMessage::Snapshot {
            cursor,
            transcript: Box::new(transcript),
        };
        for message in [welcome, snapshot] {
            write_frame(&mut writer, &message).unwrap();
        }
        writer.flush().unwrap();
        if let Ok(Some(command)) = read_frame::<_, ClientMessage>(&mut reader) {
            let ClientMessage::Command { id, .. } = &command else {
                panic!("a command, got {command:?}");
            };
            let noise = ServerMessage::Change {
                change: sheprd_chat_protocol::Change::Clients { count: 2 },
                cursor,
            };
            let ack = ServerMessage::Ack {
                id: *id,
                rejected: answer,
            };
            write_frame(&mut writer, &noise).unwrap();
            write_frame(&mut writer, &ack).unwrap();
            writer.flush().unwrap();
            heard.push(command);
        }
        heard
    })
}

#[test]
fn the_chat_is_named_by_its_variable() {
    assert!(matches!(address(None), Err(Error::NoChat)));
    for bad in ["", "not-an-id", "0123456789ABCDEF0123456789ABCDEF"] {
        assert!(
            matches!(
                address(Some(&OsString::from(bad))),
                Err(Error::InvalidEndpoint)
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn list_reads_the_chats_pull_requests_from_its_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let address = private_address(dir.path());
    let mut transcript = Transcript::new("Claude");
    let mut pull = PullRequest::parse(URL).unwrap();
    pull.actions = vec![PullRequestAction::Created];
    transcript.pull_requests = vec![pull.clone()];
    let server = chat(&address, transcript, None);
    assert_eq!(list(&address).unwrap(), [pull]);
    let heard = server.join().unwrap();
    assert!(matches!(
        heard[0],
        ClientMessage::Hello {
            client: ClientKind::Cli,
            revision: REVISION,
            since: None,
            ..
        }
    ));
}

#[test]
fn add_sends_the_address_and_waits_for_its_answer() {
    let dir = tempfile::tempdir().unwrap();
    let address = private_address(dir.path());
    let server = chat(&address, Transcript::new("Claude"), None);
    let added = add(&address, &format!(" {URL}/files ")).unwrap();
    assert_eq!(added.url, URL);
    let heard = server.join().unwrap();
    assert!(matches!(
        &heard[1],
        ClientMessage::Command {
            command: Command::AddPullRequest { url },
            ..
        } if url == URL
    ));
}

#[test]
fn a_refusal_is_reported_with_the_chats_reason() {
    let dir = tempfile::tempdir().unwrap();
    let address = private_address(dir.path());
    let server = chat(
        &address,
        Transcript::new("Claude"),
        Some(Rejection::TooManyPullRequests),
    );
    assert!(matches!(
        add(&address, URL),
        Err(Error::Rejected(Rejection::TooManyPullRequests))
    ));
    server.join().unwrap();
}

#[test]
fn a_bad_address_or_a_missing_chat_is_refused_before_anything_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let address = private_address(dir.path());
    assert!(matches!(
        add(&address, "https://example.com/o/r/pull/7"),
        Err(Error::InvalidUrl)
    ));
    assert!(matches!(add(&address, URL), Err(Error::Connect(_))));
    assert!(matches!(list(&address), Err(Error::Connect(_))));
}
