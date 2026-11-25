use super::*;

#[test]
fn slow_frames_shrink_and_quick_ones_grow_within_bounds() {
    assert_eq!(next_width(1280, Duration::from_secs(1)), 640);
    assert_eq!(next_width(1280, Duration::from_millis(300)), 1280);
    assert_eq!(next_width(1280, Duration::from_millis(50)), 1600);
    assert_eq!(next_width(200, Duration::from_secs(1)), MIN_FRAME_WIDTH);
    assert_eq!(next_width(MAX_FRAME_WIDTH, Duration::ZERO), MAX_FRAME_WIDTH);
}

#[test]
fn clicks_map_into_the_screen_and_miss_the_letterbox() {
    // A 2000x1000 screen drawn in a 1000x1000 area: 1000x500, 250 from the top.
    let area = (0., 0., 1000., 1000.);
    let screen = (2000, 1000);
    assert_eq!(screen_point((500., 500.), area, screen), Some((1000, 500)));
    assert_eq!(screen_point((0., 250.), area, screen), Some((0, 0)));
    assert_eq!(screen_point((500., 100.), area, screen), None);
    assert_eq!(screen_point((500., 900.), area, screen), None);
    assert_eq!(screen_point((10., 10.), (0., 0., 0., 0.), screen), None);
}

#[test]
fn keys_have_the_names_machine_key_takes() {
    assert_eq!(key_name("enter"), Some("return"));
    assert_eq!(key_name("backspace"), Some("delete"));
    assert_eq!(key_name("f13"), None);
}

fn typed(text: &str) -> TabOp {
    TabOp::MachineType {
        name: "default".to_owned(),
        text: text.to_owned(),
    }
}

fn key(key: &str) -> TabOp {
    TabOp::MachineKey {
        name: "default".to_owned(),
        key: key.to_owned(),
        modifiers: Vec::new(),
    }
}

#[test]
fn input_goes_in_order_one_run_at_a_time() {
    let mut inputs = Inputs::default();
    assert_eq!(inputs.push(typed("h")), Ok(true));
    assert_eq!(inputs.push(typed("i")), Ok(false));
    assert_eq!(inputs.push(key("return")), Ok(false));
    assert_eq!(inputs.push(typed("x")), Ok(false));
    assert_eq!(inputs.next(), Some(typed("hi")));
    assert_eq!(inputs.next(), Some(key("return")));
    assert_eq!(inputs.next(), Some(typed("x")));
    assert_eq!(inputs.next(), None);
    // Sending ended: the next input starts it again.
    assert_eq!(inputs.push(key("tab")), Ok(true));
}

#[test]
fn a_full_queue_drops_input() {
    let mut inputs = Inputs::default();
    for _ in 0..MAX_QUEUED {
        assert!(inputs.push(key("a")).is_ok());
    }
    assert_eq!(inputs.push(key("b")), Err(()));
}
