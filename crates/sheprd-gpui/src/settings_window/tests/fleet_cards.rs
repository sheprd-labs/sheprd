//! Settings > Fleet, the cards: a computer in no fleet offers to join the
//! fleet a computer it reaches is a member of, with Create beside it; a
//! member offers to invite every computer it reaches that is in no fleet,
//! and Not now puts that off. Neither acts before its button is clicked.
use super::fleet::{ASKED, page, serial};
use super::remote_control::{click, draw};
use super::*;
use crate::{
    device_tags::Daemon,
    fleet::{Fleet, Standing, join},
};
use sheprd_client::ConnectTarget;
use std::collections::BTreeMap;

/// A window that reaches `studio` and `m4max`, each of which said
/// `standing` when last asked, so the page does not ask them again.
fn reach(cx: &mut VisualTestContext, standing: Standing) {
    cx.update(|_, cx| {
        let window = cx.new(|_| ()).entity_id();
        let mut reachable = BTreeMap::new();
        for name in ["m4max", "studio"] {
            Fleet::set_standing(cx, name, standing);
            reachable.insert(
                name.to_owned(),
                Daemon::Remote(ConnectTarget::Ssh {
                    target: format!("me@{name}"),
                    session: "default".into(),
                }),
            );
        }
        Fleet::reach(cx, window, &reachable, &Default::default());
    });
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn a_computer_in_no_fleet_offers_to_join_the_one_it_reaches(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    reach(cx, Standing::Outside);
    // Nothing offered yet: Create, as before.
    assert!(cx.debug_bounds("fleet-join-card-0").is_none());
    assert!(cx.debug_bounds("fleet-create").is_some());
    cx.update(|_, cx| {
        let offer = join::Offer {
            fleet: "f".repeat(64),
            members: 2,
            hosts: 1,
        };
        join::set_offer(cx, "studio", Some(offer.clone()));
        // The same fleet through another computer is one card.
        join::set_offer(cx, "m4max", Some(offer));
    });
    draw(cx);
    // One card for the fleet, with a way through each member.
    assert!(cx.debug_bounds("fleet-join-card-0-join-0").is_some());
    assert!(cx.debug_bounds("fleet-join-card-0-join-1").is_some());
    assert!(cx.debug_bounds("fleet-join-card-0-create").is_some());
    assert!(cx.debug_bounds("fleet-join-card-1").is_none());
    // The card holds Create; the row it replaces is gone.
    assert!(cx.debug_bounds("fleet-create").is_none());
    assert!(ASKED.lock().unwrap().is_empty());
}

#[gpui::test]
fn a_member_offers_to_invite_every_computer_in_no_fleet(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    reach(cx, Standing::Outside);
    assert!(cx.debug_bounds("fleet-invite-card-invite").is_some());
    assert!(cx.debug_bounds("fleet-machine-0-invite").is_some());
    assert!(cx.debug_bounds("fleet-machine-1-invite").is_some());
    // Not now hides the card, and invites no one.
    click(cx, "fleet-invite-card-later");
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("fleet-invite-card").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-invite").is_some());
    assert!(ASKED.lock().unwrap().is_empty());
    assert!(!cx.update(|_, cx| Fleet::acting(cx, "studio")));
}

#[gpui::test]
fn a_computer_in_another_fleet_is_not_offered_an_invitation(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    reach(cx, Standing::Elsewhere);
    assert!(cx.debug_bounds("fleet-invite-card").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-invite").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-share").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-scrub").is_some());
}

#[gpui::test]
fn nothing_starts_while_a_join_runs(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    reach(cx, Standing::Outside);
    cx.update(|_, cx| {
        let offer = join::Offer {
            fleet: "f".repeat(64),
            members: 2,
            hosts: 1,
        };
        join::set_offer(cx, "studio", Some(offer.clone()));
        join::set_offer(cx, "m4max", Some(offer));
        join::set_acting(cx, "m4max", true);
    });
    draw(cx);
    // Neither another way in nor Create acts: a second join would leave a
    // fleet listing a computer that joined another.
    for button in ["fleet-join-card-0-join-1", "fleet-join-card-0-create"] {
        click(cx, button);
        cx.run_until_parked();
        draw(cx);
    }
    assert!(cx.debug_bounds("fleet-error").is_none());
    assert!(!cx.update(|_, cx| Fleet::acting(cx, "studio")));
    assert!(ASKED.lock().unwrap().is_empty());
    cx.update(|_, cx| join::set_acting(cx, "m4max", false));
}
