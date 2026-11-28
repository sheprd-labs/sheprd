//! A computer in no fleet, asked over SSH what it holds, answers rather
//! than failing, so Sheprd tells it from a machine without this program.

use super::*;
use crate::control::fleet::sync::Apply;

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_in_no_fleet_answers_an_empty_fleet() {
    let (http, _, _) = relay().await;
    let b = Node::new(http, 2).await;
    let asked = Apply {
        nonce: Some("nonce".to_owned()),
        ..Apply::default()
    };
    let applied = b.fleet.apply(&asked, now_ms()).unwrap();
    assert_eq!(applied, sync::Applied::default());
    // Through the daemon, as `fleet apply` asks it: still signs the nonce
    // with its CA so a member that already admitted it can recognize it.
    let Answer::Applied(applied) = b.ask(Ask::Apply(asked)) else {
        panic!("not applied");
    };
    assert!(applied.fleet.is_empty());
    assert!(applied.ops.is_empty());
    let host = b.fleet.host.unwrap().to_string();
    assert_eq!(
        sync::proven_host(&applied, "nonce", std::slice::from_ref(&host)).as_deref(),
        Some(host.as_str())
    );
    // And `Ask::Status` still reports this computer's member id.
    let Answer::Status { view, me } = b.ask(Ask::Status) else {
        panic!("not status");
    };
    assert!(view.is_none());
    assert_eq!(me.as_deref(), Some(b.me().as_str()));
}

#[tokio::test(flavor = "multi_thread")]
async fn naming_a_fleet_it_is_not_in_still_fails() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let named = Apply {
        fleet: Some(a.view().fleet),
        ..Apply::default()
    };
    assert!(b.fleet.apply(&named, now_ms()).is_err());
}

#[test]
fn admitting_over_ssh_refuses_a_phone_before_asking_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let phone = crate::web::tests::fleet::Phone::new(7);
    let (bundle, signature) = phone.join(&FleetId([1; 32]));
    let admission = sync::Admission { bundle, signature };
    // No daemon runs in `dir`: refused before one is asked.
    assert!(matches!(
        run_admit(dir.path(), admission),
        Err(Error::NotComputer)
    ));
}
