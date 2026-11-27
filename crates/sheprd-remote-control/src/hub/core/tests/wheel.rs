//! A phone's scroll on a Herdr 2 terminal goes to a full-screen program
//! that reads the wheel, as Herdr 1 routes it; a shell's view scrolls its
//! history instead.

use super::*;
use sheprd_terminal::{Emulator, Size};

fn running(output: &[u8]) -> Emulator {
    let mut emulator = Emulator::new(Size::new(40, 10).unwrap(), 100);
    emulator.feed(output);
    emulator
}

#[test]
fn a_shell_takes_no_wheel_so_its_view_scrolls_history() {
    let emulator = running(b"$ ls\r\nfile\r\n$ ");
    assert!(screen::wheel(&emulator, 3).is_empty());
    assert!(screen::wheel(&emulator, -3).is_empty());
}

#[test]
fn a_program_reading_the_mouse_gets_one_wheel_notch_per_scroll() {
    // Alternate screen, wheel reports, SGR encoding: as agents set them.
    let emulator = running(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
    assert_eq!(screen::wheel(&emulator, 3), b"\x1b[<64;21;6M");
    assert_eq!(screen::wheel(&emulator, -3), b"\x1b[<65;21;6M");
}

#[test]
fn a_full_screen_program_under_alternate_scroll_gets_arrow_keys() {
    let emulator = running(b"\x1b[?1049h\x1b[?1007h");
    assert_eq!(screen::wheel(&emulator, 1), b"\x1b[A");
    assert_eq!(screen::wheel(&emulator, -1), b"\x1b[B");
}
