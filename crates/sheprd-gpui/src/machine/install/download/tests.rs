#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    io::{BufRead as _, BufReader},
    net::TcpListener,
    sync::atomic::Ordering,
    thread::{self, JoinHandle},
    time::Duration,
};

const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// The test's own server: answers one request with what `answer` makes of
/// its headers, and hands the headers back.
fn serve(answer: impl FnOnce(&str) -> Vec<u8> + Send + 'static) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/macos.ipsw", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            headers.push_str(&line);
        }
        let mut stream = stream;
        stream.write_all(&answer(&headers)).unwrap();
        headers
    });
    (url, handle)
}

fn response(status: &str, body: &[u8], length: usize) -> Vec<u8> {
    let mut bytes =
        format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
            .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

/// Plain HTTP, which only the test's own server speaks.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into()
}

fn target(folder: &tempfile::TempDir) -> PathBuf {
    folder.path().join("25A100.ipsw")
}

#[test]
fn a_whole_download_lands_at_its_target_and_leaves_no_part() {
    let folder = tempfile::tempdir().unwrap();
    let (url, server) = serve(|_| response("200 OK", BODY, BODY.len()));
    let counter = Counter::default();
    fetch(&agent(), &url, &target(&folder), 0, &counter).unwrap();
    assert!(!server.join().unwrap().to_lowercase().contains("range:"));
    assert_eq!(fs::read(target(&folder)).unwrap(), BODY);
    assert!(!partial(&target(&folder)).exists());
    assert_eq!(counter.done.load(Ordering::Relaxed), BODY.len() as u64);
    assert_eq!(counter.total.load(Ordering::Relaxed), BODY.len() as u64);
}

#[test]
fn a_download_resumes_from_its_part() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(partial(&target(&folder)), &BODY[..10]).unwrap();
    let (url, server) = serve(|headers| {
        assert!(
            headers.to_lowercase().contains("range: bytes=10-"),
            "{headers}"
        );
        response("206 Partial Content", &BODY[10..], BODY.len() - 10)
    });
    let counter = Counter::default();
    fetch(&agent(), &url, &target(&folder), 0, &counter).unwrap();
    server.join().unwrap();
    assert_eq!(fs::read(target(&folder)).unwrap(), BODY);
    assert_eq!(counter.total.load(Ordering::Relaxed), BODY.len() as u64);
}

/// A server that ignores the range sends it all, which replaces the part.
#[test]
fn a_whole_answer_to_a_range_starts_over() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(partial(&target(&folder)), b"stale").unwrap();
    let (url, server) = serve(|_| response("200 OK", BODY, BODY.len()));
    fetch(&agent(), &url, &target(&folder), 0, &Counter::default()).unwrap();
    server.join().unwrap();
    assert_eq!(fs::read(target(&folder)).unwrap(), BODY);
}

/// A connection that drops early keeps what came, for the next try.
#[test]
fn a_cut_download_keeps_its_part_and_has_no_target() {
    let folder = tempfile::tempdir().unwrap();
    let (url, server) = serve(|_| response("200 OK", &BODY[..8], BODY.len()));
    let failed = fetch(&agent(), &url, &target(&folder), 0, &Counter::default());
    server.join().unwrap();
    assert!(failed.is_err());
    assert!(!target(&folder).exists());
    assert_eq!(fs::read(partial(&target(&folder))).unwrap(), &BODY[..8]);
}

#[test]
fn a_cancelled_download_stops_without_a_target() {
    let folder = tempfile::tempdir().unwrap();
    let (url, server) = serve(|_| response("200 OK", BODY, BODY.len()));
    let counter = Counter::default();
    counter.cancel.store(true, Ordering::Relaxed);
    let failed = fetch(&agent(), &url, &target(&folder), 0, &counter);
    server.join().unwrap();
    assert!(matches!(failed, Err(Error::Cancelled)));
    assert!(!target(&folder).exists());
}

#[test]
fn an_error_status_is_reported() {
    let folder = tempfile::tempdir().unwrap();
    let (url, server) = serve(|_| response("404 Not Found", b"", 0));
    let failed = fetch(&agent(), &url, &target(&folder), 0, &Counter::default());
    server.join().unwrap();
    assert!(matches!(failed, Err(Error::Status(404))));
}

/// A finished download is used as it is, without asking the server.
#[test]
fn a_finished_download_is_not_fetched_again() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(target(&folder), BODY).unwrap();
    let counter = Counter::default();
    fetch(
        &agent(),
        "http://127.0.0.1:9/unused",
        &target(&folder),
        0,
        &counter,
    )
    .unwrap();
    assert_eq!(counter.done.load(Ordering::Relaxed), BODY.len() as u64);
}

#[test]
fn apple_is_reached_over_https_only() {
    let folder = tempfile::tempdir().unwrap();
    let failed = fetch(
        &super::agent(),
        "http://127.0.0.1:9/macos.ipsw",
        &target(&folder),
        0,
        &Counter::default(),
    );
    assert!(matches!(failed, Err(Error::Download(_))));
}

/// A download that would leave no room for the install does not start, even
/// into a folder no try has made yet.
#[test]
fn a_download_without_room_does_not_start() {
    let root = tempfile::tempdir().unwrap();
    let folder = tempfile::TempDir::with_prefix_in("unused-", root.path()).unwrap();
    fs::remove_dir(folder.path()).unwrap();
    let (url, server) = serve(|_| response("200 OK", BODY, BODY.len()));
    let failed = fetch(
        &agent(),
        &url,
        &target(&folder),
        u64::MAX / 2,
        &Counter::default(),
    );
    server.join().unwrap();
    assert!(matches!(failed, Err(Error::NoRoom { .. })));
    assert!(!target(&folder).exists());
    assert!(!partial(&target(&folder)).exists());
}

/// A kept download is reused only with room left for the install.
#[test]
fn a_finished_download_without_room_for_the_install_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(target(&folder), BODY).unwrap();
    let failed = fetch(
        &agent(),
        "http://127.0.0.1:9/unused",
        &target(&folder),
        u64::MAX / 2,
        &Counter::default(),
    );
    assert!(matches!(failed, Err(Error::NoRoom { .. })));
    assert!(target(&folder).exists());
}

/// A whole answer replaces the part, so the part's space counts as free.
#[test]
fn starting_over_counts_the_part_it_replaces_as_room() {
    const CLAIMED: u64 = 10 << 30;
    let folder = tempfile::tempdir().unwrap();
    let Some(free) = free_bytes(folder.path()) else {
        return;
    };
    let Some(room) = free.checked_sub(4 << 30) else {
        return;
    };
    // Sparse: as long as the answer, taking no space.
    File::create(partial(&target(&folder)))
        .unwrap()
        .set_len(CLAIMED)
        .unwrap();
    let (url, server) = serve(|_| response("200 OK", BODY, CLAIMED as usize));
    let failed = fetch(&agent(), &url, &target(&folder), room, &Counter::default());
    server.join().unwrap();
    // It started over, so it reached the body, which is cut short.
    assert!(
        matches!(failed, Err(Error::Incomplete { .. } | Error::Download(_))),
        "{failed:?}"
    );
}

/// Stop ends a download whose line stalled, once its read gives up.
#[test]
fn stop_ends_a_stalled_download_and_keeps_its_part() {
    let folder = tempfile::tempdir().unwrap();
    // Sends the headers and a little, then nothing until the test ends.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/macos.ipsw", listener.local_addr().unwrap());
    let (release, released) = std::sync::mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 2 {
            line.clear();
        }
        stream
            .write_all(&response("200 OK", &BODY[..8], BODY.len()))
            .unwrap();
        let _ = released.recv();
    });
    let counter = std::sync::Arc::new(Counter::default());
    let stopper = {
        let counter = counter.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            counter.cancel.store(true, Ordering::Relaxed);
        })
    };
    let stalling = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_per_read(Some(Duration::from_millis(200)))
        .build()
        .into();
    let failed = fetch(&stalling, &url, &target(&folder), 0, &counter);
    stopper.join().unwrap();
    drop(release);
    server.join().unwrap();
    assert!(matches!(failed, Err(Error::Cancelled)), "{failed:?}");
    assert!(!target(&folder).exists());
    assert!(partial(&target(&folder)).exists());
}
