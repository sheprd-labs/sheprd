use std::time::Duration;

use sheprd_chat_protocol::{
    Cursor,
    sharing::{MAX_REQUEST_BYTES, Update},
};
use uuid::Uuid;

use crate::sharing::{
    Capability, Clock, Content, Error, GrantId, History, MAX_GRANTS, MAX_LIFETIME, MAX_SHARES,
    MAX_VIEWERS, ShareId, Source,
    tests::{Fixture, agent, clock, request},
};

#[test]
fn capability_is_scoped_to_both_share_and_grant() {
    let mut f = Fixture::new();
    let (a, guest) = f.share(History::All, Content::default());
    let (b, _) = f.share(History::All, Content::default());
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    for (share, grant, secret) in [
        (a.share, a.grant, &b.capability),
        (a.share, b.grant, &a.capability),
        (b.share, a.grant, &a.capability),
        (ShareId(Uuid::new_v4()), a.grant, &a.capability),
        (a.share, GrantId(Uuid::new_v4()), &a.capability),
    ] {
        assert!(matches!(
            f.store.admit(share, grant, secret, source, clock(0)),
            Err(Error::Denied)
        ));
    }
    assert!(f.read(&guest, None, 0).is_ok());
}

#[test]
fn revocation_removes_admitted_readers_and_does_not_revoke_other_grants() {
    let mut f = Fixture::new();
    let (a, guest) = f.share(History::All, Content::default());
    let b = f.store.invite(a.share, clock(0)).unwrap();
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    let other = f
        .store
        .admit(b.share, b.grant, &b.capability, source, clock(0))
        .unwrap();
    f.store.revoke(a.share, a.grant, clock(1)).unwrap();
    assert!(matches!(f.read(&guest, None, 1), Err(Error::Denied)));
    assert!(f.read(&other, None, 1).is_ok());
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store
            .admit(a.share, a.grant, &a.capability, source, clock(1)),
        Err(Error::Denied)
    ));
    f.store.stop(a.share, clock(2)).unwrap();
    assert!(matches!(f.read(&other, None, 2), Err(Error::Denied)));
}

#[test]
fn source_replacement_permanently_closes_the_share() {
    for field in 0..3 {
        let mut f = Fixture::new();
        let (invite, guest) = f.share(History::All, Content::default());
        let original = (f.endpoint.clone(), f.cursor, f.transcript.session.clone());
        match field {
            0 => f.endpoint = sheprd_chat_protocol::EndpointId::random(),
            1 => f.cursor = Cursor::new_epoch(),
            _ => f.transcript.session.as_mut().unwrap().id = "replacement".into(),
        }
        assert!(matches!(f.read(&guest, None, 0), Err(Error::Source)));
        (f.endpoint, f.cursor, f.transcript.session) = original;
        assert!(matches!(f.read(&guest, None, 0), Err(Error::Denied)));
        assert!(matches!(
            f.store.invite(invite.share, clock(0)),
            Err(Error::Denied)
        ));
    }
}

#[test]
fn regressed_source_sequence_closes_access() {
    let mut f = Fixture::new();
    f.push(10, agent("hello"));
    let (_, guest) = f.share(History::All, Content::default());
    f.cursor.seq = 0;
    assert!(matches!(f.read(&guest, None, 0), Err(Error::Source)));
}

#[test]
fn reconnect_and_other_readers_cannot_accept_regressed_source_sequence() {
    let mut f = Fixture::new();
    f.cursor.seq = 10;
    let (invite, guest) = f.share(History::All, Content::default());
    f.cursor.seq = 100;
    assert!(f.read(&guest, None, 1).is_ok());
    f.store.disconnect(guest);

    f.cursor.seq = 50;
    let regressed = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store.admit(
            invite.share,
            invite.grant,
            &invite.capability,
            regressed,
            clock(2)
        ),
        Err(Error::Source)
    ));
    f.cursor.seq = 100;
    let current = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store.admit(
            invite.share,
            invite.grant,
            &invite.capability,
            current,
            clock(3)
        ),
        Err(Error::Denied)
    ));

    let mut f = Fixture::new();
    f.cursor.seq = 10;
    let (invite, first) = f.share(History::All, Content::default());
    let initial = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    let second = f
        .store
        .admit(
            invite.share,
            invite.grant,
            &invite.capability,
            initial,
            clock(0),
        )
        .unwrap();
    f.cursor.seq = 100;
    assert!(f.read(&first, None, 1).is_ok());
    f.cursor.seq = 50;
    assert!(matches!(f.read(&second, None, 2), Err(Error::Source)));
    f.cursor.seq = 100;
    assert!(matches!(f.read(&first, None, 3), Err(Error::Denied)));
}

#[test]
fn expiry_checks_wall_and_monotonic_clocks_at_the_boundary() {
    for elapsed_only in [false, true] {
        let mut f = Fixture::new();
        let (_, guest) = f.share(History::All, Content::default());
        assert!(f.read(&guest, None, 59_999).is_ok());
        let mut now = clock(60_000);
        if elapsed_only {
            now.unix_ms = clock(59_999).unix_ms;
        } else {
            now.elapsed = clock(59_999).elapsed;
        }
        let source = Source {
            endpoint: &f.endpoint,
            cursor: f.cursor,
            transcript: &f.transcript,
        };
        assert!(matches!(
            f.store.read(&guest, &request(None), source, now),
            Err(Error::Denied)
        ));
    }
}

#[test]
fn either_clock_regressing_disables_every_existing_guest() {
    for wall in [false, true] {
        let mut f = Fixture::new();
        let (_, guest) = f.share(History::All, Content::default());
        f.read(&guest, None, 10).unwrap();
        let now = if wall {
            Clock {
                unix_ms: clock(9).unix_ms,
                ..clock(10)
            }
        } else {
            Clock {
                elapsed: clock(9).elapsed,
                ..clock(10)
            }
        };
        let source = Source {
            endpoint: &f.endpoint,
            cursor: f.cursor,
            transcript: &f.transcript,
        };
        assert!(matches!(
            f.store.read(&guest, &request(None), source, now),
            Err(Error::Clock)
        ));
        assert!(matches!(f.read(&guest, None, 11), Err(Error::Disabled)));
    }
}

#[test]
fn owner_commands_extra_fields_and_unknown_versions_are_rejected() {
    let mut f = Fixture::new();
    let (_, guest) = f.share(History::All, Content::default());
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    for command in [
        "prompt",
        "cancel",
        "answer_permission",
        "answer_question",
        "set_mode",
        "set_option",
        "unqueue",
        "set_auto_accept",
        "stop_activity",
        "set_agent",
        "add_pull_request",
        "shell",
        "fleet",
        "file",
        "artifact",
    ] {
        let bytes =
            serde_json::to_vec(&serde_json::json!({"version":0,"cursor":null,"command":command}))
                .unwrap();
        assert!(
            matches!(
                f.store.read(&guest, &bytes, source, clock(0)),
                Err(Error::Json(_))
            ),
            "{command}"
        );
    }
    assert!(matches!(
        f.store
            .read(&guest, br#"{"version":1,"cursor":null}"#, source, clock(0)),
        Err(Error::Version)
    ));
    assert!(matches!(
        f.store
            .read(&guest, &vec![b' '; MAX_REQUEST_BYTES + 1], source, clock(0)),
        Err(Error::Limit)
    ));
    assert!(f.read(&guest, None, 0).is_ok());
}

#[test]
fn viewer_limit_disconnect_and_kill_switch() {
    let mut f = Fixture::new();
    let (invite, first) = f.share(History::All, Content::default());
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    let mut guests = vec![first];
    for _ in 1..MAX_VIEWERS {
        guests.push(
            f.store
                .admit(
                    invite.share,
                    invite.grant,
                    &invite.capability,
                    source,
                    clock(0),
                )
                .unwrap(),
        );
    }
    assert!(matches!(
        f.store.admit(
            invite.share,
            invite.grant,
            &invite.capability,
            source,
            clock(0)
        ),
        Err(Error::Limit)
    ));
    f.store.disconnect(guests.pop().unwrap());
    assert!(
        f.store
            .admit(
                invite.share,
                invite.grant,
                &invite.capability,
                source,
                clock(0)
            )
            .is_ok()
    );
    f.store.stop_all(clock(1)).unwrap();
    for guest in guests {
        assert!(matches!(f.read(&guest, None, 1), Err(Error::Denied)));
    }
}

#[test]
fn grants_shares_lifetimes_and_missing_source_are_bounded() {
    let mut f = Fixture::new();
    let (invite, _) = f.share(History::All, Content::default());
    for _ in 1..MAX_GRANTS {
        f.store.invite(invite.share, clock(0)).unwrap();
    }
    assert!(matches!(
        f.store.invite(invite.share, clock(0)),
        Err(Error::Limit)
    ));
    for _ in 1..MAX_SHARES {
        f.share(History::All, Content::default());
    }
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store.create(
            source,
            History::All,
            Content::default(),
            MAX_LIFETIME,
            clock(0)
        ),
        Err(Error::Limit)
    ));
    for lifetime in [
        Duration::ZERO,
        Duration::from_nanos(1),
        MAX_LIFETIME + Duration::from_millis(1),
    ] {
        assert!(matches!(
            f.store
                .create(source, History::All, Content::default(), lifetime, clock(0)),
            Err(Error::Limit)
        ));
    }
    f.store.stop_all(clock(0)).unwrap();
    assert!(
        f.store
            .create(
                source,
                History::All,
                Content::default(),
                MAX_LIFETIME,
                clock(0)
            )
            .is_ok()
    );
    f.transcript.session = None;
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store.create(
            source,
            History::All,
            Content::default(),
            MAX_LIFETIME,
            clock(0)
        ),
        Err(Error::Source)
    ));
}

#[test]
fn capability_debug_and_update_have_no_secret() {
    let secret = Capability::from_bytes([37; 32]);
    assert!(!format!("{secret:?}").contains("37"));
    let mut f = Fixture::new();
    let (_, guest) = f.share(History::All, Content::default());
    assert!(matches!(
        f.read(&guest, None, 0).unwrap(),
        Update::Snapshot { .. }
    ));
}

#[test]
fn a_snapshot_older_than_sharing_and_an_unknown_history_boundary_are_refused() {
    let mut f = Fixture::new();
    f.cursor.seq = 40;
    let (invite, _) = f.share(History::FromNow, Content::default());
    f.cursor.seq = 39;
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store.admit(
            invite.share,
            invite.grant,
            &invite.capability,
            source,
            clock(0)
        ),
        Err(Error::Source)
    ));
    f.cursor.seq = 40;
    f.transcript.truncated = true;
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        f.store.create(
            source,
            History::FromNow,
            Content::default(),
            MAX_LIFETIME,
            clock(0)
        ),
        Err(Error::Source)
    ));
}
