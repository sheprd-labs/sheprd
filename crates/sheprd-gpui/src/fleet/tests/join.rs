use super::*;
use sheprd_remote_control::control::fleet::{Kind, MemberView};

fn member(id: &str) -> MemberView {
    MemberView {
        id: id.into(),
        name: id.into(),
        kind: Kind::Computer,
        admitted_by: None,
    }
}

fn machine(id: &str) -> (String, Machine) {
    (
        id.to_owned(),
        Machine {
            target: ConnectTarget::Ssh {
                target: id.to_owned(),
                session: "default".into(),
            },
            label: id.to_owned(),
        },
    )
}

#[test]
fn only_a_member_of_its_fleet_offers_it() {
    let mut view = fleet_view(&[]);
    view.members = vec![member("a"), member("b")];
    let offer = crate::fleet::join::Offer::of(&view).unwrap();
    assert_eq!(offer.fleet, view.fleet);
    assert_eq!((offer.members, offer.hosts), (2, 0));
    // A shared host holds the log but cannot admit anyone.
    view.me = "host".into();
    view.hosted = Some("host".into());
    assert_eq!(crate::fleet::join::Offer::of(&view), None);
}

#[test]
fn each_fleet_is_offered_once_with_every_member_reached() {
    let offer = |fleet: char| crate::fleet::join::Offer {
        fleet: fleet.to_string().repeat(64),
        members: 1,
        hosts: 0,
    };
    let mut fleet = Fleet::default();
    for (id, offered) in [
        ("laptop", Some(offer('a'))),
        ("studio", Some(offer('a'))),
        ("lab", Some(offer('b'))),
        ("box", None),
    ] {
        fleet.links.insert(
            id.to_owned(),
            Link {
                offer: offered,
                ..Link::default()
            },
        );
    }
    let reached: BTreeMap<_, _> = ["box", "lab", "laptop", "studio"].map(machine).into();
    let offered: Vec<_> = fleet
        .offered(reached.clone())
        .into_iter()
        .map(|offered| {
            let ids: Vec<_> = offered.through.iter().map(|(id, _)| id.clone()).collect();
            (offered.offer.fleet.chars().next(), ids)
        })
        .collect();
    assert_eq!(
        offered,
        vec![
            (Some('b'), vec!["lab".to_owned()]),
            (Some('a'), vec!["laptop".to_owned(), "studio".to_owned()]),
        ]
    );
    fleet.view = Some(fleet_view(&[]));
    assert!(fleet.offered(reached).is_empty());
}

#[test]
fn the_member_named_leads_and_the_others_keep_their_order() {
    let offered = crate::fleet::join::Offered {
        offer: crate::fleet::join::Offer {
            fleet: "a".repeat(64),
            members: 3,
            hosts: 0,
        },
        through: vec![machine("laptop"), machine("mini"), machine("studio")],
    };
    let ids = |through: Vec<(String, Machine)>| -> Vec<String> {
        through.into_iter().map(|(id, _)| id).collect()
    };
    assert_eq!(
        ids(offered.clone().through_first(Some("studio"))),
        ["studio", "laptop", "mini"]
    );
    assert_eq!(
        ids(offered.clone().through_first(None)),
        ["laptop", "mini", "studio"]
    );
    assert_eq!(
        ids(offered.through_first(Some("gone"))),
        ["laptop", "mini", "studio"]
    );
}

#[test]
fn a_machine_in_another_fleet_is_asked_rarely() {
    let (elsewhere, now) = link(Standing::Elsewhere, Duration::from_secs(600), &[]);
    assert!(!elsewhere.due(&[], now));
    let (elsewhere, now) = link(Standing::Elsewhere, OUTSIDE, &[]);
    assert!(elsewhere.due(&[], now));
}

#[test]
fn sheprd_in_no_fleet_stays_a_plain_host_of_this_one() {
    use crate::fleet::flows::beside_daemon;
    // Its relay agent belongs to this fleet: keep it current.
    assert_eq!(beside_daemon(Standing::Agent), Standing::Agent);
    assert_eq!(beside_daemon(Standing::Removed), Standing::Removed);
    // No agent of this fleet: a computer to invite.
    for plain in [Standing::NoDaemon, Standing::Outside, Standing::Unchecked] {
        assert_eq!(beside_daemon(plain), Standing::Outside);
    }
}

#[test]
fn a_reached_member_that_already_lists_this_computer_is_recognized() {
    let mut view = fleet_view(&["h1"]);
    view.me = "studio".into();
    view.members = vec![member("studio"), member("m4max")];
    let offer = crate::fleet::join::already_admitted(&view, Some("m4max")).unwrap();
    assert_eq!(offer.fleet, view.fleet);
    assert_eq!(
        crate::fleet::join::already_admitted(&view, Some("other")),
        None
    );
    assert_eq!(crate::fleet::join::already_admitted(&view, None), None);
    // A shared host (not a member itself) cannot offer or heal.
    view.me = "host".into();
    assert_eq!(
        crate::fleet::join::already_admitted(&view, Some("m4max")),
        None
    );
}

#[test]
fn status_keeps_this_computers_member_id_even_in_no_fleet() {
    let mut fleet = Fleet {
        reads: 1,
        ..Fleet::default()
    };
    assert!(fleet.answered_status(1, Ok((None, Some("m4max-id".into())))));
    assert_eq!(fleet.me.as_deref(), Some("m4max-id"));
    assert!(fleet.view.is_none());
}

#[test]
fn a_proven_live_host_with_no_log_triggers_handover_while_removed_hosts_do_not() {
    use crate::fleet::flows::{Asked, classify_answer_with};
    use sheprd_remote_control::control::fleet::sync::{Applied, Proof};

    let host = "ab".repeat(32);
    let our_fleet = "f".repeat(64);
    let applied = |fleet: &str, heads: Vec<String>, signer: Option<&str>| Applied {
        fleet: fleet.to_owned(),
        heads,
        ops: Vec::new(),
        refused: 0,
        more: false,
        proof: signer.map(|id| Proof {
            certificate: id.to_owned(),
            key: String::new(),
            signature: String::new(),
        }),
    };
    let prove = |theirs: &Applied, _nonce: &str, hosts: &[String]| {
        let id = &theirs.proof.as_ref()?.certificate;
        hosts.contains(id).then(|| id.clone())
    };
    // Live host answering with no fleet yet -> MissingLog(host).
    let empty = applied("", Vec::new(), Some(&host));
    assert_eq!(
        classify_answer_with(
            empty.clone(),
            "n1",
            &our_fleet,
            std::slice::from_ref(&host),
            &[],
            prove,
        )
        .unwrap(),
        Asked::MissingLog(host.clone())
    );
    // Removed host answering with no fleet -> NotInFleet (never handed log).
    assert!(matches!(
        classify_answer_with(
            empty.clone(),
            "n1",
            &our_fleet,
            &[],
            std::slice::from_ref(&host),
            prove,
        ),
        Err(Error::NotInFleet)
    ));
    // Stranger answering with no fleet -> NotInFleet.
    assert!(matches!(
        classify_answer_with(empty, "n1", &our_fleet, &["other".into()], &[], prove),
        Err(Error::NotInFleet)
    ));
    // Live host answering for our fleet -> Fleet.
    let joined = applied(&our_fleet, vec!["h1".into()], Some(&host));
    assert_eq!(
        classify_answer_with(
            joined.clone(),
            "n2",
            &our_fleet,
            std::slice::from_ref(&host),
            &[],
            prove,
        )
        .unwrap(),
        Asked::Fleet(joined, host.clone())
    );
    // Unproven host claiming our fleet -> Unproven.
    let unproven = applied(&our_fleet, vec!["h1".into()], None);
    assert_eq!(
        classify_answer_with(
            unproven,
            "n2",
            &our_fleet,
            std::slice::from_ref(&host),
            &[],
            prove,
        )
        .unwrap(),
        Asked::Unproven
    );
    // Another fleet -> OtherFleet.
    let elsewhere = applied(&"e".repeat(64), vec!["h2".into()], Some(&host));
    assert!(matches!(
        classify_answer_with(elsewhere, "n3", &our_fleet, &[host], &[], prove),
        Err(Error::OtherFleet)
    ));
}
