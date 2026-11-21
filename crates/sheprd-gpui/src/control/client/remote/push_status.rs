//! `remote push-status` as text: how iPhone pushes leave this computer,
//! and how the last alert and Live Activity push went. English, as all
//! command line output is.

use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

pub(super) fn text(
    enabled: bool,
    push_relay: bool,
    status: Option<&PushStatus>,
    error: Option<&str>,
) -> String {
    let setting = if push_relay { "on" } else { "off" };
    let Some(status) = status else {
        return match (enabled, error) {
            (false, _) => format!("Remote control is off; the push relay is {setting} in Settings"),
            (true, Some(error)) => format!("Remote control is on but not running: {error}"),
            (true, None) => "Remote control is on but not running".to_owned(),
        };
    };
    let now = crate::remote::now_ms();
    [
        format!("iPhone pushes: {}", mode(status.mode, push_relay)),
        format!("Push relay setting: {setting}"),
        format!("Last alert: {}", last(status.alert.as_ref(), now)),
        format!(
            "Last Live Activity push: {}",
            last(status.live_activity.as_ref(), now)
        ),
    ]
    .join("\n")
}

fn mode(mode: Mode, push_relay: bool) -> &'static str {
    match mode {
        Mode::Off => "off (no APNs key, and the push relay is off)",
        Mode::OwnKey if push_relay => "your own APNs key, which wins over the push relay",
        Mode::OwnKey => "your own APNs key",
        Mode::PushRelay => "through the push relay (push.sheprd.org)",
        Mode::Other => "a mode this Sheprd does not know",
    }
}

fn last(last: Option<&Last>, now_ms: u64) -> String {
    let Some(last) = last else {
        return "none since the daemon started".to_owned();
    };
    let ago = crate::ago::english(now_ms.saturating_sub(last.at_ms) / 1000);
    format!("{}, {ago}", outcome(&last.outcome))
}

fn outcome(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Delivered => "delivered".to_owned(),
        Outcome::Gone => {
            "the phone's registration expired; it registers again when the app opens".to_owned()
        }
        Outcome::Limited => "held back: too many pushes for that phone".to_owned(),
        Outcome::Refused {
            status,
            reason: Some(reason),
        } => format!(
            "refused ({status}, {})",
            crate::notifications::safe_text(reason, 64)
        ),
        Outcome::Refused {
            status,
            reason: None,
        } => format!("refused ({status})"),
        Outcome::Unreachable => "could not reach Apple or the push relay".to_owned(),
        Outcome::Unsent => "not sent: it could not be sealed".to_owned(),
        Outcome::Other => "an outcome this Sheprd does not know".to_owned(),
    }
}
