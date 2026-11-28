//! Handing the log to a computer with no fleet: in parts, as a member's
//! `fleet apply` does when the log is larger than one answer, and to a
//! Sheprd host the fleet shares, which is no member.

use super::*;
use crate::control::fleet::sync::Apply;

fn apply(node: &Node, ops: &[Op]) -> Answer {
    node.ask(Ask::Apply(Apply {
        ops: sync::encode_ops(ops),
        ..Apply::default()
    }))
}

/// `node`'s log in graph order, as a member hands it over.
fn in_order(node: &Node) -> Vec<Op> {
    let inner = node.fleet.inner();
    let log = inner.log.as_ref().unwrap();
    log.missing_for(&[]).into_iter().cloned().collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_sheprd_host_takes_the_log_and_lets_allowed_members_in() {
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    // Before it is shared, the host refuses the log.
    assert!(matches!(
        host.fleet.receive(a.ops(), now_ms()),
        Err(Error::NotAdmitted)
    ));
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(
        a.ask(Ask::AddHost {
            intro,
            deny: Vec::new(),
        }),
        Answer::Done
    );
    let Answer::Applied(applied) = host.ask(Ask::Apply(Apply {
        heads: a.view().heads,
        ops: sync::encode_ops(&a.ops()),
        ..Apply::default()
    })) else {
        panic!("not applied");
    };
    assert_eq!(applied.fleet, a.view().fleet);
    // It holds the log as a host, not as a member, and lets a in.
    let view = host.view();
    assert!(!view.members.iter().any(|member| member.id == host.me()));
    assert!(
        view.hosts
            .iter()
            .any(|shared| Some(shared.id.clone()) == host.fleet.host().map(|id| id.to_string()))
    );
    assert!(grants::still_allowed(&host.fleet, &a.fleet.me()));
    grants::apply(&host.state, &host.fleet).unwrap();
    assert_eq!(host.lines().len(), 1, "{:?}", host.lines());
    // It stays a host after a restart: how it joined is kept.
    let again = Fleet::load(host.dir.path(), now_ms()).unwrap();
    assert_eq!(again.host(), host.fleet.host());
    assert!(grants::still_allowed(&again, &a.fleet.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_member_never_acts_as_a_shared_host() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // b joined as a member: it has a certificate authority, but is no
    // shared host.
    assert!(b.fleet.host.is_some());
    assert_eq!(b.fleet.host(), None);
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Lost,
    });
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    let a_host = {
        let inner = a.fleet.inner();
        grants::own_host(inner.log.as_ref().unwrap().state(), &a.fleet.me(), None).unwrap()
    };
    let inner = b.fleet.inner();
    let state = inner.log.as_ref().unwrap().state();
    assert!(state.tombstones.contains(&b.fleet.me()));
    // Even handed a host the fleet holds, a removed member gets none.
    assert_eq!(grants::own_host(state, &b.fleet.me(), Some(a_host)), None);
    drop(inner);
    assert!(!grants::still_allowed(&b.fleet, &a.fleet.me()));
    grants::apply(&b.state, &b.fleet).unwrap();
    assert!(b.lines().is_empty(), "{:?}", b.lines());
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.host(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_fleets_handing_over_at_once_keep_their_own_history() {
    let (http, _, _) = relay().await;
    let (a, c, b) = (
        Node::new(http, 1).await,
        Node::new(http, 3).await,
        Node::new(http, 2).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert_eq!(c.ask(Ask::Found), Answer::Done);
    let (first, second) = (a.view().fleet, c.view().fleet);
    for (by, fleet) in [(&a, &first), (&c, &second)] {
        let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
            fleet: fleet.clone(),
        }) else {
            panic!("no bundle");
        };
        by.ask(Ask::Admit {
            bundle,
            signature,
            method: crate::control::fleet::Method::Ssh,
        });
    }
    let named = |fleet: &str, ops: &[Op], heads: Vec<String>| {
        b.fleet
            .apply(
                &Apply {
                    fleet: Some(fleet.to_owned()),
                    heads,
                    ops: sync::encode_ops(ops),
                    ..Apply::default()
                },
                now_ms(),
            )
            .unwrap()
    };
    let (a_ops, c_ops) = (in_order(&a), in_order(&c));
    // Both founding operations arrive, each answered from its own fleet's
    // history; then the rest of a's: c's history never mixes in.
    let from_a = named(&first, &a_ops[..1], Vec::new());
    let from_c = named(&second, &c_ops[..1], Vec::new());
    assert_eq!(from_a.fleet, first);
    assert_eq!(from_c.fleet, second);
    assert_eq!(b.fleet.inner().joining.len(), 2);
    let joined = named(&first, &a_ops[1..], from_a.heads);
    assert_eq!(joined.fleet, first);
    assert_eq!(joined.refused, 0);
    assert!(b.fleet.inner().joining.is_empty());
    assert_eq!(b.view().fleet, first);
}

#[tokio::test(flavor = "multi_thread")]
async fn histories_of_too_many_fleets_drop_the_oldest() {
    let (http, _, _) = relay().await;
    let b = Node::new(http, 2).await;
    let mut fleets = Vec::new();
    for seed in 10..10 + crate::fleet::joining::MAX_FLEETS as u8 + 1 {
        let founder = Node::new(http, seed).await;
        assert_eq!(founder.ask(Ask::Found), Answer::Done);
        let fleet = founder.fleet.inner().log.as_ref().unwrap().fleet();
        // Each arrives later than the one before.
        b.fleet
            .receive(founder.ops(), now_ms() + u64::from(seed))
            .unwrap_err();
        fleets.push(fleet);
    }
    let inner = b.fleet.inner();
    assert_eq!(inner.joining.len(), crate::fleet::joining::MAX_FLEETS);
    assert!(!inner.joining.contains_key(&fleets[0]));
    assert!(
        fleets[1..]
            .iter()
            .all(|fleet| inner.joining.contains_key(fleet))
    );
    drop(inner);
    // Kept apart on disk, each in its own file, across a restart.
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(
        again.inner().joining.len(),
        crate::fleet::joining::MAX_FLEETS
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invitation_cut_off_after_admission_resumes() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    assert_eq!(
        a.ask(Ask::Admit {
            bundle: bundle.clone(),
            signature: signature.clone(),
            method: crate::control::fleet::Method::Ssh,
        }),
        Answer::Done
    );
    // The log never reached b. Later, Invite or Join asks b for a fresh
    // bundle (newer rev, signed_ms, and CA signature): admitting it signs
    // nothing more, and the handover goes on.
    let heads = a.view().heads;
    let Answer::Bundle {
        bundle: fresh,
        signature: fresh_signature,
    } = ask::answer(
        &b.state,
        &b.fleet,
        Ask::Bundle {
            fleet: fleet.clone(),
        },
        now_ms() + 60_000,
    )
    .unwrap()
    else {
        panic!("no fresh bundle");
    };
    assert_ne!(fresh, bundle);
    // A forged join signature on the fresh bundle is still refused.
    assert!(
        ask::answer(
            &a.state,
            &a.fleet,
            Ask::Admit {
                bundle: fresh.clone(),
                signature: sync::text(&[0u8; 64]),
                method: crate::control::fleet::Method::Ssh,
            },
            now_ms() + 60_000,
        )
        .is_err()
    );
    assert_eq!(
        a.ask(Ask::Admit {
            bundle: fresh,
            signature: fresh_signature,
            method: crate::control::fleet::Method::Ssh,
        }),
        Answer::Done
    );
    assert_eq!(a.view().heads, heads);
    let Answer::Applied(applied) = apply(&b, &in_order(&a)) else {
        panic!("not applied");
    };
    assert_eq!(applied.fleet, fleet);
    assert!(b.view().members.iter().any(|member| member.id == b.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scrubbed_fleet_is_not_restored_by_unsolicited_apply_until_reinvited() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let fleet = a.view().fleet;
    assert_eq!(b.ask(Ask::Scrub { session: None }), Answer::Done);
    assert!(ask::view(&b.fleet).is_none());
    // Unsolicited handover from a (still listing b as live) is refused,
    // both live and across a daemon restart.
    let handover = Apply {
        fleet: Some(fleet.clone()),
        heads: a.view().heads,
        ops: sync::encode_ops(&in_order(&a)),
        ..Apply::default()
    };
    assert!(b.fleet.apply(&handover, now_ms()).is_err());
    assert!(ask::view(&b.fleet).is_none());
    let reloaded = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert!(reloaded.apply(&handover, now_ms()).is_err());
    assert!(ask::view(&reloaded).is_none());
    // An explicit re-invite/join (`Ask::Bundle`) clears the scrub guard
    // and resumes admission + handover.
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    assert_eq!(
        a.ask(Ask::Admit {
            bundle,
            signature,
            method: crate::control::fleet::Method::Ssh,
        }),
        Answer::Done
    );
    let applied = b.fleet.apply(&handover, now_ms()).unwrap();
    assert_eq!(applied.fleet, fleet);
    assert!(b.view().members.iter().any(|member| member.id == b.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_members_own_host_record_never_admits_a_different_fleet_key_as_shared() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    assert_eq!(
        a.ask(Ask::Admit {
            bundle,
            signature,
            method: crate::control::fleet::Method::Ssh,
        }),
        Answer::Done
    );
    // Replace b's fleet key while keeping its TLS CA certificate.
    std::fs::remove_file(b.dir.path().join("fleet/key")).unwrap();
    let rotated = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_ne!(rotated.me(), b.fleet.me());
    let handover = Apply {
        fleet: Some(fleet),
        heads: a.view().heads,
        ops: sync::encode_ops(&in_order(&a)),
        ..Apply::default()
    };
    // Kept in joining, never adopted as a shared host.
    let _ = rotated.apply(&handover, now_ms()).unwrap();
    assert!(rotated.inner().log.is_none());
    assert_eq!(rotated.host(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn scrubbing_an_interrupted_join_blocks_automatic_handover() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    assert_eq!(
        a.ask(Ask::Admit {
            bundle,
            signature,
            method: crate::control::fleet::Method::Ssh,
        }),
        Answer::Done
    );
    // b has no log yet (only partial history in joining); scrubbing it still
    // records an opt-out so background sync cannot adopt the fleet.
    let ops = in_order(&a);
    let partial = Apply {
        fleet: Some(fleet.clone()),
        heads: a.view().heads,
        ops: sync::encode_ops(&ops[..1]),
        ..Apply::default()
    };
    let _ = b.fleet.apply(&partial, now_ms()).unwrap();
    assert!(b.fleet.inner().log.is_none());
    assert_eq!(b.ask(Ask::Scrub { session: None }), Answer::Done);
    let handover = Apply {
        fleet: Some(fleet),
        heads: a.view().heads,
        ops: sync::encode_ops(&ops),
        ..Apply::default()
    };
    assert!(b.fleet.apply(&handover, now_ms()).is_err());
    let reloaded = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert!(reloaded.apply(&handover, now_ms()).is_err());
    assert!(ask::view(&reloaded).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn background_intro_read_does_not_clear_a_shared_hosts_scrub_guard() {
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(
        a.ask(Ask::AddHost {
            intro,
            deny: Vec::new(),
        }),
        Answer::Done
    );
    let handover = Apply {
        fleet: Some(fleet.clone()),
        heads: a.view().heads,
        ops: sync::encode_ops(&in_order(&a)),
        ..Apply::default()
    };
    assert!(host.fleet.apply(&handover, now_ms()).is_ok());
    assert_eq!(host.ask(Ask::Scrub { session: None }), Answer::Done);
    // Background `Ask::Intro` is read-only and never clears the scrub guard.
    assert!(matches!(host.ask(Ask::Intro), Answer::Intro { .. }));
    assert!(host.fleet.apply(&handover, now_ms()).is_err());
    assert!(ask::view(&host.fleet).is_none());
    // Explicit Share (`rejoin: true`) re-adopts the shared host log.
    let explicit = Apply {
        rejoin: true,
        ..handover
    };
    let applied = host.fleet.apply(&explicit, now_ms()).unwrap();
    assert_eq!(applied.fleet, fleet);
    assert!(host.fleet.host().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_explicit_share_after_scrub_resumes_on_background_sync() {
    let (http, _, _) = relay().await;
    let (a, b, host) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 5).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let fleet = a.view().fleet;
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(
        a.ask(Ask::AddHost {
            intro,
            deny: Vec::new(),
        }),
        Answer::Done
    );
    let ops = in_order(&a);
    let heads = a.view().heads;
    let full = Apply {
        fleet: Some(fleet.clone()),
        heads: heads.clone(),
        ops: sync::encode_ops(&ops),
        ..Apply::default()
    };
    assert!(host.fleet.apply(&full, now_ms()).is_ok());
    assert_eq!(host.ask(Ask::Scrub { session: None }), Answer::Done);
    // Explicit Share starts with only part 1 (`rejoin: true`), then drops.
    let (first, rest) = ops.split_at(1);
    let part1 = Apply {
        fleet: Some(fleet.clone()),
        heads: heads.clone(),
        ops: sync::encode_ops(first),
        rejoin: true,
        ..Apply::default()
    };
    let applied = host.fleet.apply(&part1, now_ms()).unwrap();
    assert_eq!(applied.fleet, fleet);
    assert!(host.fleet.inner().log.is_none());
    // Approval persists across a daemon restart and lets background sync
    // (`rejoin: false`) finish the handover.
    let reloaded = Fleet::load(host.dir.path(), now_ms()).unwrap();
    let part2 = Apply {
        fleet: Some(fleet.clone()),
        heads,
        ops: sync::encode_ops(rest),
        ..Apply::default()
    };
    let finished = reloaded.apply(&part2, now_ms()).unwrap();
    assert_eq!(finished.fleet, fleet);
    assert!(reloaded.host().is_some());
    // Host-share approval never lets a scrubbed member bypass the scrub guard.
    assert_eq!(b.ask(Ask::Scrub { session: None }), Answer::Done);
    let _ = b.fleet.apply(&part1, now_ms()).unwrap();
    assert!(b.fleet.inner().log.is_none());
    let b_reloaded = Fleet::load(b.dir.path(), now_ms()).unwrap();
    let _ = b_reloaded.apply(&part2, now_ms()).unwrap();
    assert!(b_reloaded.inner().log.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_host_waits_for_the_denials_signed_with_its_share() {
    let (http, _, _) = relay().await;
    let (a, b, host) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 5).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    // Shared with b left out.
    a.ask(Ask::AddHost {
        intro,
        deny: vec![b.me()],
    });
    let ops = in_order(&a);
    let heads = a.view().heads;
    // A part that ends with the share's addition, before its denial: the
    // host takes nothing yet, so b is never let in.
    let (first, rest) = ops.split_at(ops.len() - 1);
    assert!(matches!(first.last().unwrap().body(), Body::HostAdd(_)));
    let part = |ops: &[Op]| {
        host.fleet.apply(
            &Apply {
                fleet: Some(a.view().fleet),
                heads: heads.clone(),
                ops: sync::encode_ops(ops),
                ..Apply::default()
            },
            now_ms(),
        )
    };
    part(first).unwrap();
    assert!(host.fleet.inner().log.is_none());
    assert!(!grants::still_allowed(&host.fleet, &b.fleet.me()));
    // With the rest, the caller's whole log, it joins: a is let in, b not.
    part(rest).unwrap();
    assert!(host.fleet.inner().log.is_some());
    assert!(grants::still_allowed(&host.fleet, &a.fleet.me()));
    assert!(!grants::still_allowed(&host.fleet, &b.fleet.me()));
}
