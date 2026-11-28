use super::*;
use sheprd_chat_protocol::mux::Mux;
use sheprd_client::private_endpoint::Listener;
use std::{path::Path, time::Duration};

const ECHO: &str = "0123456789abcdef0123456789abcdef";
const GREETER: &str = "1123456789abcdef0123456789abcdef";
const MISSING: &str = "fedcba9876543210fedcba9876543210";

fn id(text: &str) -> EndpointId {
    text.parse().unwrap()
}

/// A chat endpoint that serves each connection with `serve`.
fn listen(dir: &Path, endpoint: &str, serve: fn(Connection)) {
    let listener = Listener::bind(&dir.join(format!("{endpoint}.sock"))).unwrap();
    thread::spawn(move || {
        while let Ok(connection) = listener.accept() {
            thread::spawn(move || serve(connection));
        }
    });
}

fn echo(connection: Connection) {
    let (mut reader, mut writer, _closer) = connection.split().unwrap();
    let _ = io::copy(&mut reader, &mut writer);
}

/// Says hello and hangs up, as a chat that exits does.
fn greet(connection: Connection) {
    let (_reader, mut writer, closer) = connection.split().unwrap();
    let _ = writer.write_all(b"hi");
    closer.close();
}

/// A client multiplexer talking to `serve` over pipes, as `ssh` would.
fn client(dir: &Path) -> Mux {
    let (from_client, to_host) = io::pipe().unwrap();
    let (from_host, to_client) = io::pipe().unwrap();
    let dir = dir.to_owned();
    thread::spawn(move || {
        serve(from_client, to_client, move |endpoint| {
            Some(sheprd_chat_protocol::endpoint_path(&dir, endpoint))
        })
    });
    let mux = Mux::client();
    let sender = mux.clone();
    thread::spawn(move || sender.send_loop(to_host));
    let receiver = mux.clone();
    thread::spawn(move || receiver.receive_loop(from_host));
    mux.wait_hello(Duration::from_secs(5)).unwrap();
    mux
}

#[test]
fn streams_reach_their_own_chats_and_close_alone() {
    let dir = tempfile::tempdir().unwrap();
    listen(dir.path(), ECHO, echo);
    listen(dir.path(), GREETER, greet);
    let mux = client(dir.path());

    let echoed = mux.open(&id(ECHO)).unwrap();
    let other = mux.open(&id(ECHO)).unwrap();
    echoed.clone().write_all(b"ping").unwrap();
    let mut back = [0; 4];
    echoed.clone().read_exact(&mut back).unwrap();
    assert_eq!(&back, b"ping");

    // A chat that hangs up ends only its own stream.
    let greeted = mux.open(&id(GREETER)).unwrap();
    let mut said = Vec::new();
    greeted.clone().read_to_end(&mut said).unwrap();
    assert_eq!(said, b"hi");
    greeted.close();

    // Closing one stream leaves its neighbour to the same chat running.
    echoed.close();
    other.clone().write_all(b"pong").unwrap();
    other.clone().read_exact(&mut back).unwrap();
    assert_eq!(&back, b"pong");
}

#[test]
fn an_unreachable_chat_refuses_only_its_stream() {
    let dir = tempfile::tempdir().unwrap();
    listen(dir.path(), ECHO, echo);
    let mux = client(dir.path());
    let missing = mux.open(&id(MISSING)).unwrap();
    let error = missing.clone().read(&mut [0; 1]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
    missing.close();
    let echoed = mux.open(&id(ECHO)).unwrap();
    echoed.clone().write_all(b"ok").unwrap();
    let mut back = [0; 2];
    echoed.clone().read_exact(&mut back).unwrap();
    assert_eq!(&back, b"ok");
}

#[test]
fn the_client_going_away_ends_the_multiplexer() {
    let (from_client, to_host) = io::pipe().unwrap();
    let (_from_host, to_client) = io::pipe().unwrap();
    drop(to_host);
    // Input ends at once; `serve` returns rather than waiting on.
    serve(from_client, to_client, |_| None).unwrap();
}
