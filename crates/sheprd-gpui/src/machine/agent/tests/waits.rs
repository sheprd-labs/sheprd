use super::*;
use sheprd_control_protocol::protocol::tabs::{
    MACHINE_AGENT_WAIT_SECONDS, MAX_MACHINE_TYPE_CHARS, MachineButton, TabOp,
};

/// The longest text takes the agent over 32 seconds to type, and may first
/// wait for another request's: the host waits for both, and the command line
/// waits for the host.
#[test]
fn the_longest_typing_is_waited_for_by_the_host_and_the_command_line() {
    let text = "é".repeat(MAX_MACHINE_TYPE_CHARS);
    let typing = Duration::from_millis(MACHINE_TYPE_GAP_MS * MAX_MACHINE_TYPE_CHARS as u64);
    let wait = posting(&Request::Type { text: text.clone() });
    assert!(wait >= typing * 2, "{wait:?}");
    let line = TabOp::MachineType {
        name: "a".into(),
        text,
    }
    .wait_seconds();
    assert!(Duration::from_secs(line) > CONNECT + wait + SLACK, "{line}");
}

/// A click waits behind typing under way too, within the command's wait.
#[test]
fn a_click_waits_out_typing_under_way() {
    let wait = posting(&Request::Click {
        x: 1,
        y: 1,
        button: MachineButton::Left,
        count: 1,
    });
    assert!(wait >= Duration::from_secs(MACHINE_TYPE_SECONDS));
    assert!(Duration::from_secs(MACHINE_AGENT_WAIT_SECONDS) > CONNECT + wait + SLACK);
}
