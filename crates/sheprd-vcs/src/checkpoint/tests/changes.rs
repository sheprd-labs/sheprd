//! The dialog's change history: loaded beside the list, and again after a
//! turn recorded what changed.

use super::*;

#[test]
fn opening_the_dialog_loads_the_change_history_too() {
    let mut checkpoints = Checkpoints::default();
    checkpoints.open(feature(), "repo".into(), "w4".into());
    let requests: Vec<Request> = queued(&checkpoints).into_iter().map(|(_, r)| r).collect();
    assert_eq!(requests, [Request::List, Request::History]);
    assert_eq!(checkpoints.view.as_ref().unwrap().changes, Changes::Loading);

    let history = evolution::History {
        stack: Vec::new(),
        markers: Vec::new(),
        pending: 0,
    };
    assert!(
        answer(
            &mut checkpoints,
            Request::History,
            Answer::History(Ok(Some(history.clone())))
        )
        .is_empty()
    );
    assert_eq!(
        checkpoints.view.as_ref().unwrap().changes,
        Changes::Ready(Some(history))
    );
    answer(
        &mut checkpoints,
        Request::History,
        Answer::History(Err(evolution::Error::Output.into())),
    );
    assert!(matches!(
        checkpoints.view.as_ref().unwrap().changes,
        Changes::Failed(_)
    ));
}

#[test]
fn a_turn_reloads_the_history_even_when_no_file_changed() {
    let mut checkpoints = Checkpoints::default();
    checkpoints.open(feature(), "repo".into(), "w4".into());
    checkpoints.lanes.clear();
    let mut restored = Vec::new();
    let turn = Job {
        checkout: feature(),
        request: capture("x"),
        credit: Some(evolution::Credit::Outside),
    };
    checkpoints.apply(turn, Answer::Captured(Ok(None)), &mut restored);
    assert_eq!(queued(&checkpoints), [(feature(), Request::History)]);
}
