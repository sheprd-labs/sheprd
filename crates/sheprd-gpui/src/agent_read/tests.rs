use super::*;

fn change(pane: &str, read: bool) -> Change {
    Change {
        panes: vec![pane.into()],
        read,
    }
}

fn panes(panes: &[&str]) -> BTreeSet<String> {
    panes.iter().map(|pane| (*pane).to_owned()).collect()
}

fn agent(endpoint: &str, key: &str) -> Agent {
    Agent {
        endpoint: endpoint.into(),
        key: key.into(),
    }
}

#[test]
fn unread_agents_belong_to_their_device() {
    let sets = Sets::unread(&[("local", &["p1"]), ("studio", &["p2"])]);
    assert!(sets.contains("local", "p1"));
    assert!(
        !sets.contains("local", "p2"),
        "pane ids repeat across devices"
    );
    assert!(sets.contains("studio", "p2"));
    assert!(!sets.contains("away", "p1"));
}

#[test]
fn changes_still_on_their_way_are_laid_over_an_answer() {
    let pending = [change("p1", true), change("p3", false)];
    assert_eq!(
        with_pending(panes(&["p1", "p2"]), &pending),
        panes(&["p2", "p3"])
    );
}

#[test]
fn a_host_with_no_daemon_prints_nothing_and_bad_answers_are_refused() {
    assert!(sync::parse(b"").unwrap().is_none());
    assert!(matches!(sync::parse(b"usage"), Err(Error::Answer(_))));
    let answer = sync::parse(br#"{"panes":["p1"]}"#).unwrap().unwrap();
    assert_eq!(answer.panes, panes(&["p1"]));
}

#[gpui::test]
fn a_read_shows_at_once_and_waits_for_its_daemon(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        AgentRead::install(cx);
        AgentRead::answered(
            cx,
            "studio",
            None,
            Ok(Some(Unread {
                panes: panes(&["p1", "p2"]),
            })),
        );
        assert!(AgentRead::current(cx).contains("studio", "p1"));

        AgentRead::set(cx, &agent("studio", "p1"), true);
        assert!(!AgentRead::current(cx).contains("studio", "p1"));
        let link = &cx.global::<AgentRead>().links["studio"];
        assert_eq!(link.pending, [change("p1", true)], "no daemon reached yet");

        AgentRead::set(cx, &agent("studio", " "), true);
        assert_eq!(
            cx.global::<AgentRead>().links["studio"].pending.len(),
            1,
            "a pane that is no id is not sent"
        );

        // An answer from before the daemon took the read still shows it.
        AgentRead::answered(
            cx,
            "studio",
            None,
            Ok(Some(Unread {
                panes: panes(&["p1", "p2"]),
            })),
        );
        assert!(!AgentRead::current(cx).contains("studio", "p1"));
        let sent = change("p1", true);
        AgentRead::answered(
            cx,
            "studio",
            Some(&sent),
            Ok(Some(Unread {
                panes: panes(&["p2"]),
            })),
        );
        assert!(cx.global::<AgentRead>().links["studio"].pending.is_empty());

        AgentRead::answered(cx, "studio", None, Ok(None));
        assert!(!AgentRead::current(cx).contains("studio", "p2"));
    });
}

#[gpui::test]
fn an_agent_focused_a_moment_is_read_unless_marked_unread_here(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        AgentRead::install(cx);
        let window = WindowId::from(1);
        let studio = agent("studio", "p1");
        AgentRead::answered(
            cx,
            "studio",
            None,
            Ok(Some(Unread {
                panes: panes(&["p1"]),
            })),
        );

        AgentRead::focus(cx, window, Some(studio.clone()));
        AgentRead::focus(cx, window, Some(studio.clone()));
        assert!(
            AgentRead::current(cx).contains("studio", "p1"),
            "passing over it is no read"
        );

        let settled = Instant::now().checked_sub(SETTLE * 2).unwrap();
        cx.global_mut::<AgentRead>()
            .focus
            .get_mut(&window)
            .unwrap()
            .since = settled;
        AgentRead::focus(cx, window, Some(studio.clone()));
        assert!(!AgentRead::current(cx).contains("studio", "p1"));

        // Marked unread while still focused: left so until focus moves.
        AgentRead::set(cx, &studio, false);
        AgentRead::focus(cx, window, Some(studio.clone()));
        assert!(AgentRead::current(cx).contains("studio", "p1"));

        AgentRead::focus(cx, window, None);
        assert!(cx.global::<AgentRead>().focus.is_empty());
    });
}

#[test]
fn a_failed_exchange_waits_for_the_interval_but_a_new_change_goes_at_once() {
    let interval = Duration::from_secs(5);
    let mut link = Link {
        last: Some(Instant::now()),
        failed: true,
        ..Link::default()
    };
    link.pending.push_back(change("p1", true));
    assert!(!link.due(interval));
    link.failed = false;
    assert!(link.due(interval));
    link.running = true;
    assert!(!link.due(interval));
}
