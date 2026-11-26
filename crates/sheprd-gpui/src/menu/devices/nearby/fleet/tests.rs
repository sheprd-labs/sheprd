#![allow(clippy::unwrap_used)]
use super::Ask;
use sheprd_remote_control::control::fleet::{Kind, MemberView, View, bluetooth::Hint};

const OURS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const THEIRS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn view() -> View {
    View {
        fleet: OURS.into(),
        me: "me".into(),
        members: vec![MemberView {
            id: "me".into(),
            name: "laptop".into(),
            kind: Kind::Computer,
            admitted_by: None,
        }],
        hosts: Vec::new(),
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: Vec::new(),
        hosted: None,
    }
}

fn hint(fleet: &str) -> Hint {
    Hint {
        fleet: fleet.into(),
        computers: 1,
        hosts: 0,
    }
}

#[test]
fn pairing_brings_the_two_into_the_fleet_either_has() {
    let ours = view();
    // A member invites the other computer, in none, without asking.
    assert_eq!(Ask::of(Some(&ours), true, None), Ask::Invite(hint(OURS)));
    // In no fleet, this computer joins the other's.
    assert_eq!(
        Ask::of(None, false, Some(&hint(THEIRS))),
        Ask::Join(THEIRS.into())
    );
    // Each in its own: the invitation goes, and the other pairs only.
    assert_eq!(
        Ask::of(Some(&ours), true, Some(&hint(THEIRS))),
        Ask::Invite(hint(OURS))
    );
    // Neither in a fleet: this computer founds one.
    assert_eq!(Ask::of(None, false, None), Ask::Found);
    // Already together: pairing only.
    assert_eq!(Ask::of(Some(&ours), true, Some(&hint(OURS))), Ask::Nothing);
    // A fleet held as a shared host, or after a removal, asks nothing.
    assert_eq!(
        Ask::of(Some(&ours), false, Some(&hint(THEIRS))),
        Ask::Nothing
    );
    assert_eq!(Ask::of(Some(&ours), false, None), Ask::Nothing);
}

mod page {
    use super::{OURS, THEIRS, hint};
    use crate::{
        SheprdWindow,
        fleet::Fleet,
        menu::devices::nearby::{Found, NearbySetup, Step},
        sidebar::layout_tests::fixture_window,
    };
    use gpui::TestAppContext;
    use sheprd_remote_control::control::{desktop::PairOutput, fleet::bluetooth::Hint};

    fn exchanging(view: &mut SheprdWindow, cx: &mut gpui::Context<SheprdWindow>) {
        let link = crate::menu::devices::nearby::link::input(view, cx);
        view.menu.nearby_setup = Some(NearbySetup {
            attempt: crate::menu::devices::nearby::next_attempt(),
            found: Vec::new(),
            error: None,
            pairing: None,
            radio: None,
            scanner: None,
            link,
            step: Step::Exchanging(Found {
                peripheral: "p".into(),
                name: "Studio".into(),
            }),
            _poll: None,
        });
    }

    fn code(theirs: Option<Hint>) -> PairOutput {
        PairOutput::Code {
            code: "428913".into(),
            computer: "Studio".into(),
            fleet: theirs,
            pairing: "digest".into(),
        }
    }

    #[gpui::test]
    fn the_code_shows_without_a_question(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(THEIRS))), window, cx);
            let setup = view.menu.nearby_setup.as_ref().unwrap();
            assert!(matches!(&setup.step, Step::Code { code, .. } if code == "428913"));
            assert_eq!(setup.error, None);
        });
    }

    #[gpui::test]
    fn a_bundle_the_daemon_could_not_sign_pairs_only_and_says_why(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(THEIRS))), window, cx);
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            view.nearby_claim_with(
                attempt,
                Err(crate::fleet::Error::Daemon(
                    "this computer listens on no relay".into(),
                )),
                cx,
            );
            // The pairing goes on: the page still shows the code.
            let setup = view.menu.nearby_setup.as_ref().unwrap();
            assert!(matches!(setup.step, Step::Code { .. }));
            assert_eq!(setup.error, None);
            let error = Fleet::error(cx).unwrap();
            assert!(error.contains("no relay"), "{error}");
        });
    }

    #[gpui::test]
    fn an_answer_for_a_cancelled_attempt_is_dropped(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(OURS))), window, cx);
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            view.stop_nearby_pairing(cx);
            view.nearby_claim_with(attempt, Err(crate::fleet::Error::NoDaemon), cx);
            let setup = view.menu.nearby_setup.as_ref().unwrap();
            assert_eq!(setup.step, Step::Looking);
            assert_eq!(Fleet::error(cx), None);
        });
    }

    #[gpui::test]
    fn a_claim_that_carries_the_fleet_holds_it_until_it_ends(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(THEIRS))), window, cx);
            // A join from Settings, or another pairing, waits for it.
            assert!(Fleet::busy(cx));
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            assert!(!Fleet::hold_for_pairing(cx, attempt + 1));
            // Cancelled before the other computer allowed it: free again.
            view.stop_nearby_pairing(cx);
            assert!(!Fleet::busy(cx));
        });
    }

    #[gpui::test]
    fn a_pairing_while_another_fleet_change_runs_pairs_only(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            crate::fleet::join::set_acting(cx, "studio", true);
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(THEIRS))), window, cx);
            let setup = view.menu.nearby_setup.as_ref().unwrap();
            assert!(matches!(setup.step, Step::Code { .. }));
            assert_eq!(
                Fleet::error(cx),
                Some(crate::fleet::Error::Busy.to_string())
            );
            // The running change still holds the fleet, not this pairing.
            crate::fleet::join::set_acting(cx, "studio", false);
            assert!(!Fleet::busy(cx));
        });
    }

    #[gpui::test]
    fn a_failed_or_late_answer_frees_the_fleet(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(None), window, cx);
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            view.nearby_claim_with(attempt, Err(crate::fleet::Error::NoRelays), cx);
            assert!(!Fleet::busy(cx));
            exchanging(view, cx);
            view.nearby_output(code(None), window, cx);
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            exchanging(view, cx);
            view.nearby_claim_with(attempt, Err(crate::fleet::Error::NoDaemon), cx);
            assert!(!Fleet::busy(cx));
        });
    }
}
