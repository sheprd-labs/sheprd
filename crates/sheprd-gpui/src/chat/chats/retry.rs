//! How long a chat waits before connecting again after a failure.

use super::super::Route;
use std::time::Duration;

const FIRST: Duration = Duration::from_millis(250);
/// The longest wait for a chat on this machine: its socket is cheap to try.
const LAST_LOCAL: Duration = Duration::from_secs(5);
/// The longest wait for a chat on another machine. Each attempt may cost a
/// login there, and through a relay a dial its rate limit counts, so a
/// window full of chats that cannot reach their host must not retry every
/// few seconds.
const LAST_REMOTE: Duration = Duration::from_secs(30);

/// The wait after `failures` failures in a row, doubling from [`FIRST`].
pub(super) fn delay(route: &Route, failures: u32) -> Duration {
    let last = match route {
        Route::Local => LAST_LOCAL,
        Route::Remote(_) | Route::Ssh(_) => LAST_REMOTE,
    };
    FIRST.saturating_mul(1 << failures.min(8)).min(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_chats_retry_within_seconds() {
        assert_eq!(delay(&Route::Local, 1), Duration::from_millis(500));
        assert_eq!(delay(&Route::Local, 20), LAST_LOCAL);
    }

    #[test]
    fn remote_chats_back_off_further() {
        let ssh = Route::Ssh("devbox".into());
        assert_eq!(delay(&ssh, 1), Duration::from_millis(500));
        assert_eq!(delay(&ssh, 5), Duration::from_secs(8));
        assert_eq!(delay(&ssh, 7), LAST_REMOTE);
        assert_eq!(delay(&ssh, u32::MAX), LAST_REMOTE);
    }
}
