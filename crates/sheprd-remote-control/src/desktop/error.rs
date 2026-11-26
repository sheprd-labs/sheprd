use sheprd_relay::{Endpoint, client};
use std::path::PathBuf;

/// Why adding a nearby Sheprd, or reaching one through a relay, failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the connection was cancelled")]
    Cancelled,
    #[error("the other computer did not take the claim: {0}")]
    Claim(String),
    #[error("the other computer's answer is not what a claim answers")]
    ClaimFormat,
    #[error("the codes did not match; pair again")]
    CodeMismatch,
    #[error("the other computer's user did not allow it")]
    Denied,
    #[error("nobody answered on the other computer in time")]
    Expired,
    #[error("could not {operation} {}", path.display())]
    File {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the request to the other computer failed")]
    Http(#[from] hyper::Error),
    #[error("the request to the other computer could not be written")]
    HttpRequest(#[from] hyper::http::Error),
    #[error("the code exchange did not check out; pair again")]
    Nearby(#[from] crate::auth::Error),
    #[error("the other computer's answer over Bluetooth is not one Sheprd reads")]
    NearbyFormat,
    #[error("the other computer answered over Bluetooth: {0}")]
    NearbyRefused(String),
    #[error("the other computer answered with no way to log in over SSH")]
    NoLogin,
    #[error("the other computer listens on no relays; turn on Remote control with relays there")]
    NoRelays,
    #[error(transparent)]
    PairingLink(#[from] crate::control::pairing_link::Error),
    #[error("{0} is not a machine this computer added")]
    Record(String),
    #[error("another machine is already saved under {0}; remove it before pairing this one")]
    RecordConflict(String),
    #[error("the relay record is not valid")]
    RecordFormat,
    #[error("{endpoint}: {source}")]
    Relay {
        endpoint: String,
        #[source]
        source: client::Error,
    },
    #[error("This relay has banned your network address{0}")]
    RelayBanned(sheprd_relay::ban::Ban),
    #[error("the other computer's relay details are not valid")]
    RelayInfo,
    #[error("could not start the async runtime")]
    Runtime(#[source] std::io::Error),
    #[error("the connection through the relay ended")]
    Splice(#[source] std::io::Error),
    #[error("ssh-keygen could not make this computer's SSH key")]
    SshKeygen(#[source] Option<std::io::Error>),
    #[error("the TLS connection to the other computer failed")]
    Tls(#[source] std::io::Error),
    #[error("the end-to-end tunnel to the other computer failed")]
    Tunnel(#[from] sheprd_tunnel::Error),
    // keep-sorted end
}

impl Error {
    /// Keep an explicit ban visible if later relays also fail.
    pub(super) fn record_relay(&mut self, endpoint: &Endpoint, source: client::Error) {
        if !matches!(
            self,
            Self::Relay {
                source: client::Error::Banned(_),
                ..
            }
        ) {
            *self = Self::Relay {
                endpoint: endpoint.to_string(),
                source,
            };
        }
    }

    /// The relay ban that stopped this, if one did.
    pub(super) fn ban(&self) -> Option<sheprd_relay::ban::Ban> {
        match self {
            Self::Relay {
                source: client::Error::Banned(ban),
                ..
            }
            | Self::RelayBanned(ban) => Some(*ban),
            _ => None,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use crate::desktop::Error;
    use sheprd_relay::{Endpoint, client};

    #[test]
    fn a_later_offline_relay_preserves_the_confirmed_ban_and_its_endpoint()
    -> Result<(), sheprd_relay::endpoint::Error> {
        let first = Endpoint::parse("https://first.example")?;
        let second = Endpoint::parse("https://second.example")?;
        let mut failure = Error::NoRelays;
        failure.record_relay(&first, client::Error::Banned(Default::default()));
        failure.record_relay(
            &second,
            client::Error::Refused(sheprd_relay::wire::Status::Offline),
        );
        assert!(failure.to_string().contains("first.example"));
        assert!(failure.to_string().contains("banned your network address"));
        Ok(())
    }
}
