#![allow(clippy::unwrap_used)]
use super::*;
use std::{cell::Cell, fs};

#[test]
fn a_ready_event_queue_cannot_starve_a_scheduled_full_scan() {
    let home = tempfile::tempdir().unwrap();
    let busy = home.path().join("busy.jsonl");
    let missed = home.path().join("missed.jsonl");
    fs::write(
        &busy,
        "{\"type\":\"user\",\"message\":{\"content\":\"busy\"}}\n",
    )
    .unwrap();
    let roots = [Root {
        agent: sheprd_client::herdr::AgentKind::Claude,
        dir: home.path().to_owned(),
    }];
    let host = Host {
        id: Host::id_of("watch-test"),
        label: "box".into(),
        user: "me@box".into(),
        os: "test".into(),
    };
    let mut store = Store::in_memory().unwrap();
    let stop = AtomicBool::new(false);
    let (sender, events) = mpsc::channel();
    for _ in 0..100 {
        sender
            .send(Ok(
                notify::Event::new(notify::EventKind::Any).add_path(busy.clone())
            ))
            .unwrap();
    }
    // Only the prequeued busy-file notifications reach the loop. The clock
    // advances explicitly, so neither OS delivery nor sleeps decide the test.
    let follower = Follower {
        watcher: notify::recommended_watcher(|_: notify::Result<notify::Event>| {}).unwrap(),
        watched: vec![false],
        parents: BTreeSet::new(),
        aliases: Vec::new(),
    };
    let clock = Cell::new(0u64);
    let start = Instant::now();
    let mut first = true;
    let mut found = false;
    watch_loop(
        Scan {
            store: &mut store,
            host: &host,
            roots: &roots,
            redactor: &Redactor::new(None),
            stop: &stop,
        },
        follower,
        events,
        || 0,
        |store, _| {
            if first {
                first = false;
                fs::write(
                    &missed,
                    "{\"type\":\"user\",\"message\":{\"content\":\"missed\"}}\n",
                )
                .unwrap();
            }
            found = store
                .sessions(None, 10)
                .unwrap()
                .iter()
                .any(|session| session.title.as_deref() == Some("missed"));
            if found {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
        || {
            let seconds = clock.get();
            clock.set(seconds + 1);
            if seconds >= 120 {
                stop.store(true, Ordering::Relaxed);
            }
            start + Duration::from_secs(seconds)
        },
    )
    .unwrap();
    assert!(
        found,
        "the busy queue prevented discovery of a file with no event"
    );
}

mod aliases;
#[cfg(target_os = "linux")]
mod reads;
