use super::*;

#[test]
fn only_claude_and_codex_in_a_herdr_1_pane_are_observed() {
    assert!(observable("w1:p1", Some("claude")));
    assert!(observable("w1:p1", Some("codex")));
    assert!(!observable("w1:p1", Some("bash")));
    assert!(!observable("w1:p1", None));
}

#[cfg(unix)]
mod running {
    use super::*;

    /// A chat whose "observer" only waits, so nothing ever arrives.
    fn idle(pane: &str) -> Arc<Chat> {
        let mut command = sheprd_client::new_command("/bin/sleep");
        command.arg("30");
        Chat::observe(pane.to_owned(), command).unwrap()
    }

    fn binding(pane: &str, session: &str) -> Binding {
        Binding {
            pane: pane.to_owned(),
            agent: Agent::Claude,
            session: session.to_owned(),
            pid: 10,
            started: 20,
        }
    }

    #[test]
    fn the_same_conversation_reuses_its_observer() {
        let observed = Observed::default();
        let chat = idle("w1:p1");
        observed.keep(binding("w1:p1", "a"), chat.clone());
        let found = observed.find(&binding("w1:p1", "a")).unwrap();
        assert!(Arc::ptr_eq(&found, &chat));
        observed.retain(|_| false);
        assert!(chat.is_closed());
    }

    #[test]
    fn another_session_in_the_pane_ends_the_old_observer() {
        let observed = Observed::default();
        let old = idle("w1:p1");
        observed.keep(binding("w1:p1", "a"), old.clone());
        assert!(observed.find(&binding("w1:p1", "b")).is_none());
        assert!(old.is_closed());
    }

    #[test]
    fn past_the_limit_the_oldest_unfollowed_observer_ends() {
        let observed = Observed::default();
        let followed = idle("w0:p1");
        let (_, _phone) = followed.subscribe();
        observed.keep(binding("w0:p1", "a"), followed.clone());
        let chats: Vec<_> = (1..=MAX_OBSERVED)
            .map(|n| {
                let pane = format!("w{n}:p1");
                let chat = idle(&pane);
                observed.keep(binding(&pane, "a"), chat.clone());
                chat
            })
            .collect();
        assert!(!followed.is_closed(), "a phone still follows it");
        assert!(chats[0].is_closed());
        assert!(chats[1..].iter().all(|chat| !chat.is_closed()));
        observed.retain(|_| false);
    }
}

#[cfg(unix)]
mod lifecycle {
    use super::*;

    fn observer(program: &str, args: &[&str]) -> Arc<Chat> {
        let mut command = sheprd_client::new_command(program);
        command.args(args);
        Chat::observe("w1:p1".to_owned(), command).unwrap()
    }

    #[tokio::test]
    async fn an_observer_that_ends_before_reading_is_not_opened() {
        let chat = observer("/usr/bin/true", &[]);
        assert!(!chat.opened(Duration::from_secs(5)).await);
    }

    #[tokio::test]
    async fn an_observer_that_reads_nothing_in_time_is_not_opened() {
        let chat = observer("/bin/sleep", &["30"]);
        assert!(!chat.opened(Duration::from_millis(200)).await);
        chat.close();
    }

    #[test]
    fn the_newest_observer_is_kept_past_the_limit_when_all_others_are_followed() {
        let observed = Observed::default();
        let mut phones = Vec::new();
        for n in 0..MAX_OBSERVED {
            let chat = observer("/bin/sleep", &["30"]);
            phones.push(chat.subscribe().1);
            observed.keep(
                Binding {
                    pane: format!("w{n}:p1"),
                    agent: Agent::Claude,
                    session: "a".into(),
                    pid: 1,
                    started: 1,
                },
                chat,
            );
        }
        let newest = observer("/bin/sleep", &["30"]);
        observed.keep(
            Binding {
                pane: "new:p1".into(),
                agent: Agent::Claude,
                session: "a".into(),
                pid: 1,
                started: 1,
            },
            newest.clone(),
        );
        assert!(!newest.is_closed());
        observed.retain(|_| false);
    }

    #[test]
    fn a_kept_observer_that_never_reads_is_discarded_and_ended() {
        let observed = Observed::default();
        let starting = observer("/bin/sleep", &["30"]);
        let other = observer("/bin/sleep", &["30"]);
        let binding = |pane: &str| Binding {
            pane: pane.into(),
            agent: Agent::Claude,
            session: "a".into(),
            pid: 1,
            started: 1,
        };
        // Kept as it starts, so a phone leaving mid-wait leaves it bounded.
        observed.keep(binding("w1:p1"), starting.clone());
        observed.keep(binding("w2:p1"), other.clone());
        observed.discard(&starting);
        assert!(starting.is_closed());
        assert!(!other.is_closed());
        assert_eq!(observed.sessions(), [("w2:p1".to_owned(), "a".to_owned())]);
        observed.retain(|_| false);
    }

    #[test]
    fn ending_a_session_closes_only_its_observer() {
        let observed = Observed::default();
        let old = observer("/bin/sleep", &["30"]);
        let binding = Binding {
            pane: "w1:p1".into(),
            agent: Agent::Claude,
            session: "old".into(),
            pid: 1,
            started: 1,
        };
        observed.keep(binding, old.clone());
        observed.end("w1:p1", "other");
        assert!(!old.is_closed());
        observed.end("w1:p1", "old");
        assert!(old.is_closed());
        assert!(observed.sessions().is_empty());
    }
}
