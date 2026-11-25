#![allow(clippy::unwrap_used)]
use super::*;

fn quiet() -> Guide {
    Guide {
        quiet: true,
        from_image: true,
        ..Guide::default()
    }
}

/// A machine made from an image that was set up already never shows its
/// guide: the agent's first answer closes it.
#[test]
fn a_set_up_image_settles_a_quiet_guide() {
    let mut guide = quiet();
    assert_eq!(guide.answered(Some((true, true))), Answered::Settled);
}

/// An image that still needs a step shows the guide at that step.
#[test]
fn a_quiet_guide_shows_at_the_step_the_agent_reports() {
    let mut guide = quiet();
    assert_eq!(
        guide.answered(Some((true, false))),
        Answered::Changed { ready: false }
    );
    assert!(!guide.quiet);
    assert_eq!(
        Step::of(guide.signed_in, guide.permissions, false),
        Step::ScreenRecording
    );
}

/// No answer yet keeps it out of sight, until the machine has run for
/// [`QUIET`].
#[test]
fn a_quiet_guide_shows_once_the_agent_stays_silent() {
    let mut guide = quiet();
    let start = Instant::now();
    assert_eq!(guide.answered(None), Answered::Same);
    assert!(!guide.waited(false, start));
    assert!(guide.running_since.is_none());
    assert!(!guide.waited(true, start));
    assert!(!guide.waited(true, start + QUIET / 2));
    assert!(guide.quiet);
    assert!(guide.waited(true, start + QUIET));
    assert!(!guide.quiet);
    assert!(!guide.waited(true, start + QUIET * 2));
}

/// Time the machine spends stopped or suspended does not count: the wait
/// starts over once it runs again.
#[test]
fn a_stop_starts_the_wait_over() {
    let mut guide = quiet();
    let start = Instant::now();
    assert!(!guide.waited(true, start));
    assert!(!guide.waited(true, start + QUIET / 2));
    assert!(!guide.waited(false, start + QUIET / 2));
    assert!(guide.running_since.is_none());
    let resumed = start + QUIET * 3;
    assert!(!guide.waited(true, resumed));
    assert!(!guide.waited(true, resumed + QUIET / 2));
    assert!(guide.quiet);
    assert!(guide.waited(true, resumed + QUIET));
}

/// An agent slower than [`QUIET`] still settles the guide its wait showed,
/// as its first answer speaks for the image.
#[test]
fn a_late_first_answer_still_settles() {
    let mut guide = quiet();
    let start = Instant::now();
    assert!(!guide.waited(true, start));
    assert!(guide.waited(true, start + QUIET));
    assert_eq!(guide.answered(Some((true, true))), Answered::Settled);
}

/// Once the agent has answered short of set up, the user is going through
/// the steps: finishing them shows the last one rather than closing.
#[test]
fn a_guide_that_saw_a_missing_step_does_not_settle() {
    let mut guide = quiet();
    assert_eq!(
        guide.answered(Some((false, false))),
        Answered::Changed { ready: false }
    );
    assert_eq!(
        guide.answered(Some((true, true))),
        Answered::Changed { ready: true }
    );
}

/// A guide shown at once moves on with the agent, and notes the machine
/// ready the first time both permissions are there.
#[test]
fn a_shown_guide_marks_ready_once() {
    let mut guide = Guide::default();
    assert_eq!(
        guide.answered(Some((false, false))),
        Answered::Changed { ready: false }
    );
    assert_eq!(
        guide.answered(Some((true, true))),
        Answered::Changed { ready: true }
    );
    assert_eq!(guide.answered(Some((true, true))), Answered::Same);
}

/// Asking for a quiet guide shows it; a quiet opening of a shown one does not
/// hide it.
#[gpui::test]
fn asking_shows_a_quiet_guide(cx: &mut gpui::TestAppContext) {
    let name = Name::try_from("guide-quiet".to_owned()).unwrap();
    cx.update(|cx| {
        begin(&name, Opening::Quiet, cx);
        assert_eq!(shown(&name), None);
        begin(&name, Opening::Quiet, cx);
        assert_eq!(shown(&name), None);
        begin(&name, Opening::Shown, cx);
        assert_eq!(shown(&name).map(|shown| shown.step), Some(Step::SignIn));
        let settles = with(&name, |guide| guide.answered(Some((true, true))));
        assert_eq!(settles, Some(Answered::Changed { ready: true }));
        begin(&name, Opening::Quiet, cx);
        assert!(shown(&name).is_some());
        forget(&name);
    });
}
