//! Why a relay banned an address, and for how long, as it tells the
//! banned client: in the HTTP headers of its 403 ([`REASON_HEADER`],
//! [`SECONDS_HEADER`]) and in the reason bytes of the QUIC close
//! ([`Ban::encode`]). A client reads both with [`Ban::parse`], and
//! `relay-connect` hands one to `ssh`'s caller on a [`LINE_PREFIX`] line.
//!
//! The words are a closed set: anything else parses as an unknown reason,
//! so nothing the relay or a middlebox writes reaches a user as text.

use std::{fmt, time::Duration};

/// On a 403 marked banned, why ([`BanReason::label`]).
pub const REASON_HEADER: &str = "sheprd-relay-ban-reason";
/// On a 403 marked banned, seconds until the ban lifts; absent for a ban
/// that lasts until the operator lifts it.
pub const SECONDS_HEADER: &str = "sheprd-relay-ban-seconds";
/// The start of the stderr line `relay-connect` writes for a ban, followed
/// by [`Ban::encode`]'s text.
pub const LINE_PREFIX: &str = "sheprd-relay-ban:";

/// What earned a ban.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BanReason {
    /// Key proofs that did not check out.
    BadProof,
    /// More bytes in an hour than the relay's transfer budget.
    Bandwidth,
    /// Too many connections at once.
    ConnectionCap,
    /// Over the relay's rate limits.
    Limited,
    /// Bytes that are not the relay's protocol.
    Malformed,
    /// The relay's operator, by hand.
    Manual,
    /// Connections held open without a request.
    Probe,
}

impl BanReason {
    pub const ALL: [Self; 7] = [
        Self::BadProof,
        Self::Bandwidth,
        Self::ConnectionCap,
        Self::Limited,
        Self::Malformed,
        Self::Manual,
        Self::Probe,
    ];

    /// The word the relay sends.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BadProof => "bad_proof",
            Self::Bandwidth => "bandwidth",
            Self::ConnectionCap => "connection_cap",
            Self::Limited => "limited",
            Self::Malformed => "malformed",
            Self::Manual => "manual",
            Self::Probe => "probe",
        }
    }

    /// The reason `label` names, if it is one.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.label() == label)
    }
}

impl fmt::Display for BanReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadProof => "key proofs that did not check out",
            Self::Bandwidth => "moving more data than the relay allows in an hour",
            Self::ConnectionCap => "too many connections at once",
            Self::Limited => "going over the relay's rate limits",
            Self::Malformed => "data that is not the relay's protocol",
            Self::Manual => "a decision of the relay's operator",
            Self::Probe => "connections held open without a request",
        })
    }
}

/// A ban as the relay describes it; either part may be unknown, from an
/// older relay or a stream reset that carries no reason.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Ban {
    pub reason: Option<BanReason>,
    /// How long until it lifts; `None` when it lasts or is not known.
    pub remaining: Option<Duration>,
}

/// The longest wait a ban may claim: the relay's longest earned ban is a
/// day, so a larger number is not one it sent.
const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;

impl Ban {
    /// `REASON [SECONDS]`, the text of a QUIC close and of a stderr line.
    #[must_use]
    pub fn encode(&self) -> String {
        let reason = self.reason.map_or("unknown", BanReason::label);
        match self.remaining {
            Some(remaining) => format!("{reason} {}", remaining.as_secs()),
            None => reason.to_owned(),
        }
    }

    /// Reads [`Self::encode`]'s text. Unknown words leave that part unknown.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut words = text.split_ascii_whitespace();
        let reason = words.next().unwrap_or_default();
        Self::from_parts(reason, words.next().unwrap_or_default())
    }

    /// A ban from its reason's label and its seconds, as the headers carry
    /// them. Unknown words leave that part unknown.
    #[must_use]
    pub fn from_parts(reason: &str, seconds: &str) -> Self {
        Self {
            reason: BanReason::from_label(reason.trim()),
            remaining: seconds
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|seconds| *seconds <= MAX_SECONDS)
                .map(Duration::from_secs),
        }
    }

    /// The ban on a [`LINE_PREFIX`] line of `stderr`, the last if several.
    #[must_use]
    pub fn from_stderr(stderr: &str) -> Option<Self> {
        stderr
            .lines()
            .rev()
            .find_map(|line| line.trim().strip_prefix(LINE_PREFIX))
            .map(Self::parse)
    }

    /// Whole minutes until it lifts, rounded up; `None` when not known.
    #[must_use]
    pub fn minutes_left(&self) -> Option<u64> {
        self.remaining
            .map(|remaining| remaining.as_secs().div_ceil(60).max(1))
    }
}

impl fmt::Display for Ban {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.reason, self.minutes_left()) {
            (Some(reason), Some(minutes)) => {
                write!(f, " for {reason}; it lifts in about {minutes} min")
            }
            (Some(reason), None) => write!(f, " for {reason}"),
            (None, Some(minutes)) => write!(f, "; it lifts in about {minutes} min"),
            (None, None) => Ok(()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{Ban, BanReason, LINE_PREFIX};
    use std::time::Duration;

    #[test]
    fn every_reason_round_trips_through_its_label() {
        for reason in BanReason::ALL {
            assert_eq!(BanReason::from_label(reason.label()), Some(reason));
        }
        assert_eq!(BanReason::from_label("rude"), None);
    }

    #[test]
    fn a_ban_round_trips_and_unknown_parts_stay_unknown() {
        let ban = Ban {
            reason: Some(BanReason::Probe),
            remaining: Some(Duration::from_secs(540)),
        };
        assert_eq!(Ban::parse(&ban.encode()), ban);
        let lasting = Ban {
            reason: Some(BanReason::Manual),
            remaining: None,
        };
        assert_eq!(Ban::parse(&lasting.encode()), lasting);
        assert_eq!(Ban::parse(""), Ban::default());
        assert_eq!(Ban::parse("<b>hi</b> 99999999999"), Ban::default());
    }

    #[test]
    fn the_last_ban_line_in_stderr_is_read() {
        let stderr = format!(
            "relay-connect: noise\n{LINE_PREFIX} malformed 60\n{LINE_PREFIX} probe 61\nkex_exchange_identification: Connection closed by remote host\n"
        );
        let ban = Ban::from_stderr(&stderr).expect("a ban line");
        assert_eq!(ban.reason, Some(BanReason::Probe));
        assert_eq!(ban.minutes_left(), Some(2));
        assert_eq!(Ban::from_stderr("Connection closed by UNKNOWN\n"), None);
    }

    #[test]
    fn a_ban_explains_itself_after_the_banned_sentence() {
        let ban = Ban {
            reason: Some(BanReason::Bandwidth),
            remaining: Some(Duration::from_secs(3_600)),
        };
        assert_eq!(
            ban.to_string(),
            " for moving more data than the relay allows in an hour; it lifts in about 60 min"
        );
        assert_eq!(Ban::default().to_string(), "");
    }
}
