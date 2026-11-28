//! An SSH host's multiplexed chats and the chats that dial on their own
//! count against separate limits.

use super::*;
use crate::chat::existing::Binding;

const RELAY: &str = "penso@r-c83c5296e5b87858.sheprd.invalid";

/// A native chat in `native`, and an observed existing agent in each of
/// `observed`, all on the relay host.
fn targets(native: &str, observed: &[String]) -> Vec<Target> {
    let route = Route::Ssh(RELAY.into());
    let mut targets = discovery::targets(&snapshot(&[(native, ID)]), Some(&route));
    targets.extend(observed.iter().map(|pane| Target {
        endpoint: ID.parse().unwrap(),
        existing: Some(Binding {
            pane_id: pane.clone(),
            agent: sheprd_chat_protocol::Agent::Codex,
            session_id: "12345678-1234-4123-8123-123456789abc".into(),
            herdr_session: None,
            pid: 21,
            shell_pid: 20,
            started_ticks: 123,
        }),
        pane_id: pane.clone(),
        route: route.clone(),
        surface: Surface::Herdr,
    }));
    targets
}

#[test]
fn observers_keep_to_the_channels_the_multiplexer_leaves() {
    let observed: Vec<String> = (0..20).map(|n| format!("o{n}")).collect();
    let focused = observed[19].clone();
    let kept = discovery::bounded_with(
        targets("native", &observed),
        |pane| u8::from(pane != focused),
        |_| true,
    );
    let direct = kept
        .iter()
        .filter(|target| target.existing.is_some())
        .count();
    // Two shared logins carry 16 channels, and the multiplexer takes one.
    assert_eq!(direct, 15);
    assert!(kept.iter().any(|target| target.pane_id == "native"));
    // The focused observer displaces one in the background.
    assert!(kept.iter().any(|target| target.pane_id == focused));
}

#[test]
fn without_a_multiplexer_every_chat_shares_the_channels() {
    let observed: Vec<String> = (0..20).map(|n| format!("o{n}")).collect();
    let kept = discovery::bounded_with(targets("native", &observed), |_| 3, |_| false);
    assert_eq!(kept.len(), 16);
}
