//! A host that cannot be read keeps awake only what it was itself doing.
use super::*;

fn observe_endpoints(
    cx: &mut gpui::TestAppContext,
    endpoints: &[(&'static str, Work)],
    reports: &Reports,
) {
    cx.update(|cx| observe(window(1), endpoints.iter().copied(), cx, reports.sink()));
    cx.run_until_parked();
}

fn until_agents_finish(cx: &mut gpui::TestAppContext, log: &Log, reports: &Reports) {
    cx.update(|cx| toggle_until_agents_finish_with(cx, holder(log), reports.sink()));
    cx.run_until_parked();
}

#[gpui::test]
fn a_host_that_never_connects_does_not_keep_the_hold(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_endpoints(
        cx,
        &[("local", Work::Working), ("offline", Work::Unknown)],
        &reports,
    );
    until_agents_finish(cx, &log, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));

    observe_endpoints(
        cx,
        &[("local", Work::Idle), ("offline", Work::Unknown)],
        &reports,
    );
    assert_eq!(cup_of(cx), Cup::Off);
    assert_eq!(commands(&log), [Command::Hold, Command::Release]);
    assert!(reports.take().is_empty());
}

#[gpui::test]
fn a_host_that_never_connects_is_nothing_to_wait_for(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_endpoints(
        cx,
        &[("local", Work::Working), ("offline", Work::Unknown)],
        &reports,
    );
    observe_endpoints(
        cx,
        &[("local", Work::Idle), ("offline", Work::Unknown)],
        &reports,
    );
    until_agents_finish(cx, &log, &reports);

    assert_eq!(cup_of(cx), Cup::Off);
    assert!(log.entries().is_empty());
    assert!(matches!(
        reports.take().as_slice(),
        [crate::Error::Caffeine(Error::NoWorkingAgents)]
    ));
}

#[gpui::test]
fn a_host_that_drops_while_working_keeps_the_hold(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_endpoints(
        cx,
        &[("local", Work::Idle), ("remote", Work::Working)],
        &reports,
    );
    until_agents_finish(cx, &log, &reports);

    // Its agents are likely still running while it reconnects.
    observe_endpoints(
        cx,
        &[("local", Work::Idle), ("remote", Work::Unknown)],
        &reports,
    );
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));

    observe_endpoints(
        cx,
        &[("local", Work::Idle), ("remote", Work::Idle)],
        &reports,
    );
    assert_eq!(cup_of(cx), Cup::Off);
    assert!(reports.take().is_empty());
}

#[gpui::test]
fn a_disabled_host_stops_counting(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_endpoints(
        cx,
        &[("local", Work::Idle), ("remote", Work::Working)],
        &reports,
    );
    until_agents_finish(cx, &log, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));

    observe_endpoints(cx, &[("local", Work::Idle)], &reports);
    assert_eq!(cup_of(cx), Cup::Off);
    assert_eq!(commands(&log), [Command::Hold, Command::Release]);
}
