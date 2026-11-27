//! Which surface carries a Tailscale check: the prompt over the box the tabs
//! fill is the one that shows whether or not a terminal exists, so the card
//! over the terminal stands down while a check is pending. The check also
//! reaches the window as a sentence rather than as a typed failure — the
//! transport reports `PermissionDenied` — and a surface reading only the
//! failure would offer nothing for it, which is what these tests pin.

#![allow(clippy::unwrap_used, clippy::expect_used)]

// Named imports, not `super::*`: the parent pulls `gpui` in with a glob, and a
// glob-imported macro of the same name would take a plain `#[test]` over.
use super::SheprdWindow;
use crate::{LiveState, sidebar::layout_tests, state::ConnectionStatus};
use gpui::{TestAppContext, VisualTestContext, px, size};
use sheprd_client::{
    SshFailure, TailscaleCheck,
    protocol::{FrameData, PaneSurfaceFrame},
};
use std::sync::Arc;

const URL: &str = "https://login.tailscale.com/a/o84652-335960037";

/// GPUI double-buffers debug bounds; draw both buffers before asking.
fn draw(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// The sentence a `PermissionDenied` refusal leaves, in the shape the client
/// writes it: the address sits mid-line, with advice after it.
fn refusal_sentence() -> String {
    format!(
        "Tailscale SSH wants you to verify your identity: open {URL} to authenticate, then try again"
    )
}

/// A connection with a frame on screen, and then lost: the window keeps the
/// picture, dimmed, which is what puts the card over the terminal at all.
fn lose_a_presented_frame(view: &mut SheprdWindow) {
    let snapshot = Arc::new(layout_tests::snapshot(1));
    let mut live = LiveState::default();
    live.status = ConnectionStatus::Connected;
    live.surface = Some(Arc::new(PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: snapshot.revision,
        frame: FrameData {
            cells: Vec::new(),
            width: 0,
            height: 0,
            cursor: None,
            hyperlinks: Vec::new(),
            graphics: Vec::new(),
        },
        panes: Vec::new(),
        splits: Vec::new(),
        popup: None,
        graphics: Default::default(),
    }));
    live.snapshot = Some(snapshot);
    view.presentation.frame(&live);
    view.presentation.hold();
}

fn pending(view: &mut SheprdWindow, failure: Option<SshFailure>, error: Option<String>) {
    let endpoint = &mut view.endpoints[0];
    endpoint.live.ssh_failure = failure;
    endpoint.live.error = error;
    endpoint.observe_tailscale();
}

#[gpui::test]
fn a_pending_check_has_the_prompt_and_not_the_card(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(layout_tests::fixture_window);
    view.update(cx, |view, _| {
        lose_a_presented_frame(view);
        assert!(view.reconnecting(), "a lost picture puts the card up");
        pending(
            view,
            Some(SshFailure::Tailscale(
                TailscaleCheck::from_address(URL).unwrap(),
            )),
            None,
        );
    });
    view.update(cx, |view, cx| {
        assert!(view.awaiting_tailscale());
        assert!(
            view.render_reconnecting(cx).is_none(),
            "the card must not be a second, dimmer copy of the prompt"
        );
        assert!(
            view.render_tailscale_prompt().is_some(),
            "the address is the one thing the user can act on"
        );
    });
}

/// The same check, arriving the way the transport actually reports it.
#[gpui::test]
fn a_check_the_transport_reported_as_text_is_still_a_check(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(layout_tests::fixture_window);
    view.update(cx, |view, _| pending(view, None, Some(refusal_sentence())));
    view.update(cx, |view, _| {
        assert!(view.awaiting_tailscale(), "the sentence names a check");
        assert!(view.render_tailscale_prompt().is_some());
    });
}

/// Nothing waits: the card is free to say why the host dropped, and the prompt
/// stays out of the way.
#[gpui::test]
fn without_a_check_the_card_speaks_for_itself(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(layout_tests::fixture_window);
    view.update(cx, |view, _| lose_a_presented_frame(view));
    view.update(cx, |view, cx| {
        assert!(!view.awaiting_tailscale());
        assert!(view.render_reconnecting(cx).is_some());
        assert!(view.render_tailscale_prompt().is_none());
    });
}

/// The prompt dims the box the tabs fill and centres in it, not the window:
/// the status bar's own item for the same check stays outside the dimming, and
/// the terminal under the prompt stays inside it.
#[gpui::test]
fn the_prompt_dims_the_tabs_box_and_not_the_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(layout_tests::fixture_window);
    view.update(cx, |view, _| {
        pending(
            view,
            Some(SshFailure::Tailscale(
                TailscaleCheck::from_address(URL).unwrap(),
            )),
            None,
        );
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    let dim = cx.debug_bounds("tailscale-dim").expect("the dim draws");
    let terminal = cx.debug_bounds("terminal").expect("a terminal to dim");
    let item = cx
        .debug_bounds("status-tailscale")
        .expect("the bar's own item for the same check");
    let card = cx
        .debug_bounds("tailscale-prompt")
        .expect("the prompt draws");
    assert!(
        dim.contains(&terminal.center()),
        "the dim must cover the terminal it replaces: {terminal:?} is not in {dim:?}"
    );
    assert!(
        !dim.contains(&item.center()),
        "the status bar's item for the same check stays legible: {item:?} is in {dim:?}"
    );
    assert!(
        !dim.intersects(&item),
        "no part of the status bar falls under the dimming: {item:?} meets {dim:?}"
    );
    assert!(
        dim.contains(&card.center()),
        "the prompt sits in the dimmed box: {card:?} is not in {dim:?}"
    );
    // Centred on the box, not on the window: a card the whole window centres
    // would still land inside this dim, so the centre itself is what is read.
    let off = card.center() - dim.center();
    assert!(
        off.x.abs() <= px(1.) && off.y.abs() <= px(1.),
        "the prompt is centred on the tabs box: {card:?} is {off:?} off the centre of {dim:?}"
    );
}
