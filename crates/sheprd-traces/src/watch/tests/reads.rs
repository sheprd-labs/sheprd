use super::*;
use std::{fs, io::Write};

/// Collects the paths of the OS events that arrive within `wait`.
fn drain(
    events: &mpsc::Receiver<notify::Result<notify::Event>>,
    roots: &[Root],
    wait: Duration,
) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    while let Ok(event) = events.recv_timeout(wait) {
        collect(event, roots, &[], &mut paths);
    }
    paths
}

#[test]
fn reading_a_session_file_does_not_queue_it_again() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().canonicalize().unwrap();
    let session = dir.join("session.jsonl");
    fs::write(
        &session,
        "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n",
    )
    .unwrap();
    let roots = [Root {
        agent: sheprd_client::herdr::AgentKind::Claude,
        dir: dir.clone(),
    }];
    let (sender, events) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(sender).unwrap();
    watcher.watch(&dir, RecursiveMode::Recursive).unwrap();

    // The indexer's own read must not count as a change, or every file it
    // reads is read again forever.
    fs::read(&session).unwrap();
    assert_eq!(
        drain(&events, &roots, Duration::from_millis(300)),
        BTreeSet::new()
    );

    let mut file = fs::OpenOptions::new().append(true).open(&session).unwrap();
    file.write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"again\"}}\n")
        .unwrap();
    drop(file);
    assert_eq!(
        drain(&events, &roots, Duration::from_millis(300)),
        BTreeSet::from([session])
    );
}
