use super::*;
use sheprd_backend::RunState;
use sheprd_client::protocol::AgentStatus;

fn info(name: &str) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: None,
        foreground: None,
        id: TerminalId::new(name).unwrap(),
        labels: Default::default(),
        name: None,
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

/// Delivers `list` as the answer to a read already on its way.
fn answer(infos: &mut Infos, list: Vec<TerminalInfo>) -> bool {
    let (send, receive) = sync_channel(1);
    send.send(Listed {
        core: Some(Ok(list)),
        ..Default::default()
    })
    .unwrap();
    let asked = Instant::now();
    infos.pending = Some((asked, receive));
    infos.last = Some(asked);
    infos.poll(None, None, Instant::now())
}

#[test]
fn a_started_terminal_is_known_at_once() {
    let mut infos = Infos::default();
    let started = infos.insert(info("t1"));
    assert_eq!(infos.get(&started.id), Some(&started));
}

#[test]
fn a_started_terminal_outlives_one_list_that_missed_it() {
    let mut infos = Infos::default();
    infos.insert(info("t2"));
    // A list asked for before the start names only the older terminal.
    assert!(answer(&mut infos, vec![info("t1")]));
    assert!(infos.get(&TerminalId::new("t2").unwrap()).is_some());
    assert!(infos.get(&TerminalId::new("t1").unwrap()).is_some());
    // The next one is the truth: a terminal it does not name is gone.
    assert!(answer(&mut infos, vec![info("t1")]));
    assert!(infos.get(&TerminalId::new("t2").unwrap()).is_none());
}

#[test]
fn a_list_is_dated_by_when_it_was_asked_for() {
    let mut infos = Infos::default();
    assert_eq!(infos.read_at(), None);
    let (send, receive) = sync_channel(1);
    send.send(Listed {
        core: Some(Ok(Vec::new())),
        ..Default::default()
    })
    .unwrap();
    let asked = Instant::now() - Duration::from_secs(3);
    infos.pending = Some((asked, receive));
    infos.last = Some(Instant::now());
    infos.poll(None, None, Instant::now());
    assert_eq!(infos.read_at(), Some(asked));
}

#[cfg(feature = "tmux")]
#[test]
fn a_failed_core_list_does_not_hide_tmux_changes() {
    let mut infos = Infos::default();
    let tmux = sheprd_tmux::Tmux::named_id("secondary").unwrap();
    infos.list_for_test(vec![info("t1")]);
    let (send, receive) = sync_channel(1);
    send.send(Listed {
        core: Some(Err(sheprd_tmux::Error::Invalid.into())),
        tmux: Some(Ok(vec![info(tmux.as_str())])),
    })
    .unwrap();
    infos.pending = Some((Instant::now(), receive));
    assert!(infos.poll(None, None, Instant::now()));
    assert!(infos.get(&tmux).is_some());
    assert_eq!(
        infos
            .core_terminals()
            .map(|info| info.id.as_str())
            .collect::<Vec<_>>(),
        ["t1"]
    );
    assert!(infos.get(&TerminalId::new("t1").unwrap()).is_some());
}
