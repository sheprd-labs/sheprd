//! The pane menu's plugin rows and a highlight: only the menu of the pane
//! that holds the highlight lists the actions that take one, only those
//! actions are sent it, and it is fenced by the frame on screen when the
//! row runs.

use super::*;
use crate::plugin_actions::{Target, tests::fake};
use sheprd_client::scrollback::{SelectionReadParams, TextPoint};

/// Opens the menu of the pane under grid cell `(column, row)` and lets the
/// stand-in host's answer land.
fn open_at(
    view: &gpui::Entity<SheprdWindow>,
    cx: &mut gpui::VisualTestContext,
    column: f32,
    row: f32,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            let position = mouse_position(view, column, row);
            view.open_pane_menu_at(position, window, cx);
            assert!(view.menu.page.is_some());
        })
    });
    cx.run_until_parked();
}

/// Presses Up `ups` times from no selection, so the `ups`th plugin row from
/// the end, then Enter; returns the action run, its pane, and its selection.
fn run_from_end(
    view: &gpui::Entity<SheprdWindow>,
    cx: &mut gpui::VisualTestContext,
    ups: usize,
) -> (String, Target, Option<SelectionReadParams>) {
    let key = |key: &str| gpui::KeyDownEvent {
        keystroke: crate::keymap::parse_keystroke(key).unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for _ in 0..ups {
                view.pane_menu_key(&key("up"), window, cx);
            }
            view.pane_menu_key(&key("enter"), window, cx);
        })
    });
    cx.run_until_parked();
    let mut run = fake::take_invocations();
    assert_eq!(run.len(), 1);
    let invocation = run.remove(0);
    (
        format!("{}/{}", invocation.plugin_id, invocation.action_id),
        invocation.target,
        invocation.selection,
    )
}

#[gpui::test]
fn only_the_highlighted_panes_menu_offers_and_sends_its_selection(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("plugin-menu-selection");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare(view, endpoint, cx);
            // A highlight in the focused pane, w1:p1.
            select(view, (3., 2.), (7., 3.), true);
            fake::install(view);
            fake::answer(fake::hunk_and_notes());
        })
    });
    let other = Target::Pane("w1:p2".into());
    let highlighted = Target::Pane("w1:p1".into());

    // The other pane's menu lists Hunk's review and Notes' append only: no
    // selection rows, and no selection sent even to an action that takes one.
    open_at(&view, cx, 50., 5.);
    assert_eq!(
        run_from_end(&view, cx, 1),
        ("notes/append".into(), other.clone(), None)
    );
    open_at(&view, cx, 50., 5.);
    assert_eq!(
        run_from_end(&view, cx, 2),
        ("hunk/review-pane".into(), other, None)
    );

    // The highlighted pane's menu adds Hunk's selection action. Output while
    // it is open moves the pane to revision 9; the action is fenced with it.
    open_at(&view, cx, 10., 5.);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].content_revision = 9;
        })
    });
    let selection = SelectionReadParams {
        pane_id: "w1:p1".into(),
        anchor: TextPoint { row: 1, col: 2 },
        cursor: TextPoint { row: 2, col: 6 },
        content_revision: Some(9),
    };
    assert_eq!(
        run_from_end(&view, cx, 2),
        (
            "hunk/send-selection".into(),
            highlighted.clone(),
            Some(selection.clone())
        )
    );
    open_at(&view, cx, 10., 5.);
    assert_eq!(
        run_from_end(&view, cx, 1),
        ("notes/append".into(), highlighted.clone(), Some(selection))
    );
    // An action that takes no selection is not sent one.
    open_at(&view, cx, 10., 5.);
    assert_eq!(
        run_from_end(&view, cx, 3),
        ("hunk/review-pane".into(), highlighted, None)
    );
}
