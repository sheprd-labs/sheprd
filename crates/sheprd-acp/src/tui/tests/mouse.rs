#![allow(clippy::unwrap_used, clippy::expect_used)]

use crossterm::event::{KeyModifiers, MouseEvent};

use super::*;

fn mouse(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn only_wheel_mouse_events_reach_the_redrawing_loop() {
    assert!(terminal_event_is_relevant(&mouse(MouseEventKind::ScrollUp)));
    assert!(terminal_event_is_relevant(&mouse(
        MouseEventKind::ScrollDown
    )));
    assert!(!terminal_event_is_relevant(&mouse(MouseEventKind::Moved)));
    assert!(!terminal_event_is_relevant(&mouse(MouseEventKind::Down(
        crossterm::event::MouseButton::Left,
    ))));
    assert!(terminal_event_is_relevant(&Event::FocusGained));
}
