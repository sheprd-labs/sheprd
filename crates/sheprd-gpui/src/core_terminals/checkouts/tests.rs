use crate::core_terminals::checkouts::*;

#[test]
fn folders_are_asked_once_then_again_when_stale() {
    let mut checkouts = Checkouts::default();
    let now = Instant::now();
    let folder = std::env::temp_dir().display().to_string();
    checkouts.poll(vec![folder.clone()], now);
    let asking = checkouts.asking.take().unwrap();
    let answers = asking.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(answers.folders.len(), 1);
    assert_eq!(
        answers.home,
        sheprd_client::local_path::home_dir().map(|home| home.to_string_lossy().into_owned())
    );
    // A finished batch lands on the next poll.
    let (send, receive) = mpsc::sync_channel(1);
    send.send(Answers {
        folders: vec![Answer {
            cwd: folder.clone(),
            identity: Lookup::Ready(None),
            upstream: None,
        }],
        home: None,
    })
    .unwrap();
    checkouts.asking = Some(receive);
    assert!(checkouts.poll(vec![folder.clone()], now));
    assert!(
        checkouts.asking.is_none(),
        "fresh answers are not asked again"
    );
    assert!(checkouts.get(&folder).is_none(), "not a checkout");
    checkouts.poll(vec![folder.clone()], now + REFRESH);
    assert!(checkouts.asking.is_some(), "stale answers are");
}

#[test]
fn a_folder_no_longer_listed_is_forgotten() {
    let mut checkouts = Checkouts::default();
    let identity = Identity {
        branch: Some("main".into()),
        key: "/src/app/.git".into(),
        label: "app".into(),
        linked: false,
        root: "/src/app".into(),
    };
    checkouts.seed("/src/app", Some(identity));
    assert_eq!(checkouts.get("/src/app").unwrap().label, "app");
    assert!(checkouts.poll(Vec::new(), Instant::now()));
    assert!(checkouts.get("/src/app").is_none());
}

#[test]
fn home_answers_are_cached_per_host_and_survive_a_failed_lookup() {
    fn receive(checkouts: &mut Checkouts, home: Option<&str>) -> bool {
        let (send, receiver) = mpsc::sync_channel(1);
        send.send(Answers {
            folders: Vec::new(),
            home: home.map(str::to_owned),
        })
        .unwrap();
        checkouts.asking = Some(receiver);
        checkouts.poll(Vec::new(), Instant::now())
    }
    let mut local = Checkouts::default();
    let mut remote = Checkouts::default();
    assert!(receive(&mut local, Some("/Users/alice")));
    assert!(receive(&mut remote, Some("/home/alice")));
    assert_eq!(local.home(), Some("/Users/alice"));
    assert_eq!(remote.home(), Some("/home/alice"));
    assert!(!receive(&mut remote, None));
    assert_eq!(remote.home(), Some("/home/alice"));
    assert!(!receive(&mut remote, Some("/home/alice")));
}

#[test]
fn one_answer_keeps_home_identity_and_upstream_counts_together() {
    let mut checkouts = Checkouts::default();
    let folder = "/work/repo/src".to_owned();
    let now = Instant::now();
    checkouts.asked = Some(now);
    for counts in [Some((3, 2)), None] {
        let (send, receive) = mpsc::sync_channel(1);
        send.send(Answers {
            home: Some("/home/alice".into()),
            folders: vec![Answer {
                cwd: folder.clone(),
                upstream: counts,
                identity: Lookup::Ready(Some(Identity {
                    branch: Some("feature".into()),
                    key: "/work/repo/.git".into(),
                    label: "repo".into(),
                    linked: false,
                    root: "/work/repo".into(),
                })),
            }],
        })
        .unwrap();
        checkouts.asking = Some(receive);
        assert!(checkouts.poll(vec![folder.clone()], now));
        assert_eq!(checkouts.home(), Some("/home/alice"));
        assert_eq!(checkouts.get(&folder).unwrap().root, "/work/repo");
        assert_eq!(checkouts.upstream(&folder), counts);
    }
}

#[test]
fn failures_keep_checkout_names_but_confirmed_plain_folders_clear_them() {
    let now = Instant::now();
    let cwd = "/work/repo/src";
    for linked in [false, true] {
        let mut checkouts = Checkouts::default();
        checkouts.seed(
            cwd,
            Some(Identity {
                branch: Some("feature".into()),
                key: "/work/repo/.git".into(),
                label: "repo".into(),
                linked,
                root: "/work/repo".into(),
            }),
        );
        for (identity, remains) in [(Lookup::Unavailable, true), (Lookup::Ready(None), false)] {
            let (send, receive) = mpsc::sync_channel(1);
            send.send(Answers {
                home: None,
                folders: vec![Answer {
                    cwd: cwd.into(),
                    identity,
                    upstream: None,
                }],
            })
            .unwrap();
            checkouts.asking = Some(receive);
            checkouts.poll(vec![cwd.into()], now);
            if remains {
                let retained = checkouts.get(cwd).unwrap();
                assert_eq!(retained.root, "/work/repo");
                assert_eq!(retained.linked, linked);
            } else {
                assert!(checkouts.get(cwd).is_none());
            }
            assert!(
                checkouts.asking.is_none(),
                "failures retain the normal retry interval"
            );
        }
    }
}
