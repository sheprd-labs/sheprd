//! `sheprd_client::ssh::Error` and the `SshFailure` it classifies a closed
//! bridge by, from `error-client-ssh.ftl`.

use super::Localize;
use crate::i18n::fl;
use sheprd_client::{SshFailure, ssh::Error};
use sheprd_relay::ban::{Ban, BanReason};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Cancelled => fl!("error-client-ssh-cancelled"),
            Self::Closed => fl!("error-client-ssh-closed"),
            Self::InvalidSocket => fl!("error-client-ssh-invalid-socket"),
            Self::InvalidTarget => fl!("error-client-ssh-invalid-target"),
            Self::KnownHostsUnavailable => fl!("error-client-ssh-known-hosts-unavailable"),
            Self::OutputLimit => fl!("error-client-ssh-output-limit"),
            Self::Refused(failure) => {
                fl!("error-client-ssh-refused", error = failure.localize())
            }
            Self::Timeout => fl!("error-client-ssh-timeout"),
            Self::Unchecked => fl!("error-client-ssh-unchecked"),
            Self::Unsupported => fl!("error-client-ssh-unsupported"),
            // keep-sorted end
        }
    }
}

impl Localize for SshFailure {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Auth => fl!("error-client-ssh-failure-auth"),
            Self::HerdrMissing => fl!("error-client-ssh-failure-herdr-missing"),
            Self::HostKey => fl!("error-client-ssh-failure-host-key"),
            Self::LocalNetworkDenied => fl!("error-client-ssh-failure-local-network-denied"),
            Self::NoRoute => fl!("error-client-ssh-failure-no-route"),
            Self::Other => fl!("error-client-ssh-failure-other"),
            Self::RelayBanned(ban) => ban.localize(),
            Self::Tailscale(check) => match check.url() {
                Some(url) => fl!("error-client-ssh-failure-tailscale-url", url = url),
                None => fl!("error-client-ssh-failure-tailscale"),
            },
            Self::Unreachable => fl!("error-client-ssh-failure-unreachable"),
            // keep-sorted end
        }
    }
}

impl Localize for Ban {
    fn localize(&self) -> String {
        let ban = match self.reason {
            Some(reason) => fl!(
                "error-client-ssh-failure-relay-banned-for",
                reason = reason.localize()
            ),
            None => fl!("error-client-ssh-failure-relay-banned"),
        };
        match self.minutes_left() {
            Some(minutes) => fl!(
                "error-client-ssh-failure-relay-banned-lifts",
                ban = ban,
                minutes = minutes
            ),
            None => ban,
        }
    }
}

impl Localize for BanReason {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::BadProof => fl!("error-client-ssh-ban-reason-bad-proof"),
            Self::Bandwidth => fl!("error-client-ssh-ban-reason-bandwidth"),
            Self::ConnectionCap => fl!("error-client-ssh-ban-reason-connection-cap"),
            Self::Limited => fl!("error-client-ssh-ban-reason-limited"),
            Self::Malformed => fl!("error-client-ssh-ban-reason-malformed"),
            Self::Manual => fl!("error-client-ssh-ban-reason-manual"),
            Self::Probe => fl!("error-client-ssh-ban-reason-probe"),
            // keep-sorted end
        }
    }
}
