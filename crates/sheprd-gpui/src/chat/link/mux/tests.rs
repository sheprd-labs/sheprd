use super::*;
use sheprd_chat_protocol::mux::MAX_STREAMS;
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

const ID: &str = "0123456789abcdef0123456789abcdef";

fn endpoint() -> EndpointId {
    ID.parse().unwrap()
}

fn shell(script: &str) -> Command {
    let mut command = sheprd_client::new_command("/bin/sh");
    command.args(["-c", script]);
    command
}

/// A client multiplexer greeted by an echoing host over pipes.
fn echoing() -> Mux {
    let (client, host) = (Mux::client(), Mux::host(MAX_STREAMS));
    let (from_client, to_host) = std::io::pipe().unwrap();
    let (from_host, to_client) = std::io::pipe().unwrap();
    for (mux, reader, writer) in [
        (client.clone(), from_host, to_host),
        (host.clone(), from_client, to_client),
    ] {
        let receiver = mux.clone();
        thread::spawn(move || receiver.receive_loop(reader));
        thread::spawn(move || mux.send_loop(writer));
    }
    thread::spawn(move || {
        while let Some((_, stream)) = host.accept() {
            thread::spawn(move || {
                let _ = std::io::copy(&mut stream.clone(), &mut stream.clone());
                stream.close();
            });
        }
    });
    client.wait_hello(Duration::from_secs(5)).unwrap();
    client
}

#[test]
fn chats_on_one_host_share_one_multiplexer() {
    let registry = Registry::default();
    let starts = AtomicUsize::new(0);
    let start = || {
        starts.fetch_add(1, Ordering::SeqCst);
        Ok(echoing())
    };
    let first = registry.open("host", &endpoint(), 0, start).unwrap();
    let second = registry.open("host", &endpoint(), 0, start).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_ne!(first.id(), second.id());
    second.clone().write_all(b"hi").unwrap();
    let mut back = [0; 2];
    second.clone().read_exact(&mut back).unwrap();
    assert_eq!(&back, b"hi");
    // Another host gets its own.
    registry.open("other", &endpoint(), 0, start).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[test]
fn a_host_that_refuses_falls_back_without_asking_again() {
    let registry = Registry::default();
    let starts = AtomicUsize::new(0);
    let refuse = || {
        starts.fetch_add(1, Ordering::SeqCst);
        Err(Failed::Refused)
    };
    assert!(registry.open("old", &endpoint(), 0, refuse).is_none());
    assert!(registry.open("old", &endpoint(), 0, refuse).is_none());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn an_ended_multiplexer_is_started_again() {
    let registry = Registry::default();
    let starts = AtomicUsize::new(0);
    let start = || {
        starts.fetch_add(1, Ordering::SeqCst);
        Ok(echoing())
    };
    let stream = registry.open("host", &endpoint(), 0, start).unwrap();
    {
        let hosts = registry.hosts.lock().unwrap();
        let Slot::Up { mux, .. } = &*hosts["host"].lock().unwrap() else {
            panic!("no multiplexer");
        };
        mux.shutdown();
    }
    assert_eq!(stream.clone().read(&mut [0; 1]).unwrap(), 0);
    assert!(registry.open("host", &endpoint(), 0, start).is_some());
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[test]
fn an_older_program_is_told_from_an_unreachable_host() {
    // An older `sheprd-acp` takes `multiplex` for an agent and exits 2.
    let refused = spawn(shell("echo 'unknown agent' >&2; exit 2"), ()).unwrap_err();
    assert!(matches!(refused, Failed::Refused));
    // Output that is no greeting, such as a login banner, is no multiplexer.
    let chatter = spawn(shell("echo welcome; exit 0"), ()).unwrap_err();
    assert!(matches!(chatter, Failed::Refused));
    // `ssh` itself failing says nothing about the host's program.
    let unreachable = spawn(shell("exit 255"), ()).unwrap_err();
    assert!(matches!(unreachable, Failed::Unreachable));
}

#[test]
fn a_greeting_host_multiplexes() {
    // A host's greeting by hand, then it waits for frames. The shell stays
    // to hold its stdout open: `exec cat >/dev/null` would close the only
    // writer, and the multiplexer would end on that end of file, racing
    // the check below.
    let hello = r"\001\000\000\000\000\020\000\000\000SHEPRDMX\001\000\000\000\100\000\000\000";
    let mux = spawn(shell(&format!("printf '{hello}'; cat >/dev/null")), ()).unwrap();
    assert!(!mux.is_ended());
    mux.shutdown();
}

#[test]
fn a_reconnected_host_gets_a_new_multiplexer() {
    let registry = Registry::default();
    let starts = AtomicUsize::new(0);
    let start = || {
        starts.fetch_add(1, Ordering::SeqCst);
        Ok(echoing())
    };
    let before = registry.open("host", &endpoint(), 0, start).unwrap();
    // The target may name another machine now: new chats go there.
    let after = registry.open("host", &endpoint(), 1, start).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    // A chat attached before keeps its stream on the old one.
    before.clone().write_all(b"old").unwrap();
    let mut back = [0; 3];
    before.clone().read_exact(&mut back).unwrap();
    assert_eq!(&back, b"old");
    after.clone().write_all(b"new").unwrap();
    after.clone().read_exact(&mut back).unwrap();
    assert_eq!(&back, b"new");
    assert!(registry.open("host", &endpoint(), 1, start).is_some());
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}
