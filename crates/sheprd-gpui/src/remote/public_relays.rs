//! Sheprd's own relays, for Settings to offer: the list at
//! `sheprd.org/relays.json`, taken only when one of the signers compiled
//! into `sheprd_relay_directory::signed` vouched for its exact bytes. A
//! list nobody signed, or an expired one, offers nothing.

mod error;

pub use error::Error;
use sheprd_relay_directory::{
    self as directory, Directory,
    signed::{self, Signatures, Signer},
};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// How long the list and its signatures may take, each.
const TIMEOUT: Duration = Duration::from_secs(10);
/// The newest list's serial, kept so an older signed list, which may name
/// a retired certificate's pin, is refused rather than taken.
const SERIAL_FILE: &str = "public-relays-serial.json";
/// A serial is a number; anything longer is not this file.
const SERIAL_MAX_BYTES: u64 = 64;

/// The relays Sheprd lists, and who vouched for the list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Listed {
    /// When the list stops being taken, in Unix seconds.
    pub(crate) expires: u64,
    /// Its serial: no list with a lower one is taken after it.
    pub(crate) serial: u64,
    pub(crate) relays: Vec<Public>,
    pub(crate) signer: Signer,
}

impl Listed {
    /// Whether the list still offers its relays at `now` (Unix seconds).
    pub(crate) fn expired(&self, now: u64) -> bool {
        now >= self.expires
    }
}

/// Now, in Unix seconds.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// One of Sheprd's relays.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Public {
    /// Its address, as `[remote] relays` and the daemon's status name it.
    pub(crate) address: String,
    /// A name to show, such as `Frankfurt`.
    pub(crate) name: Option<String>,
    /// A short region code, such as `fra1`.
    pub(crate) region: Option<String>,
    /// The SHA-256 of its QUIC certificate, signed with the list: once
    /// added, the daemon trusts only that certificate there.
    pub(crate) quic_pin: Option<[u8; 32]>,
}

/// Downloads the list and its signatures, and checks them, refusing one
/// older than the newest taken before. Blocking: run it off the UI thread.
pub(crate) fn fetch() -> Result<Listed, Error> {
    let path = crate::preferences::state_dir().map(|dir| dir.join(SERIAL_FILE));
    let kept = path.as_deref().and_then(kept_serial);
    let listed = download_list(kept)?;
    if let Some(path) = &path
        && kept.is_none_or(|kept| listed.serial > kept)
        && let Err(error) = crate::state_file::write(path, &listed.serial)
    {
        tracing::warn!(%error, "Could not keep the relay list's serial");
    }
    Ok(listed)
}

/// The serial kept at `path`, if one was. A damaged file keeps nothing, as
/// before any list was taken.
pub(crate) fn kept_serial(path: &Path) -> Option<u64> {
    let bytes = crate::state_file::read(path, SERIAL_MAX_BYTES)
        .inspect_err(|error| tracing::warn!(%error, "Could not read the relay list's serial"))
        .ok()??;
    serde_json::from_slice(&bytes).ok()
}

/// Where [`fetch`] keeps the serial, for tests.
#[cfg(test)]
pub(crate) fn serial_path(dir: &Path) -> std::path::PathBuf {
    dir.join(SERIAL_FILE)
}

fn download_list(kept: Option<u64>) -> Result<Listed, Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .build()
        .into();
    let bytes = download(&agent, directory::URL, directory::MAX_BYTES)?.ok_or(Error::Missing)?;
    let openpgp = download(&agent, signed::OPENPGP_URL, signed::MAX_SIGNATURE);
    let ed25519 = download(&agent, signed::ED25519_URL, signed::MAX_SIGNATURE);
    settle(&bytes, openpgp, ed25519, now(), kept)
}

/// What the list offers given how each signature's download went. One
/// signer is enough, so a signature that could not be downloaded counts as
/// absent while the other vouches for the list; only when no signature
/// that arrived does is the failed download the reason reported.
pub(crate) fn settle(
    bytes: &[u8],
    openpgp: Result<Option<Vec<u8>>, Error>,
    ed25519: Result<Option<Vec<u8>>, Error>,
    now: u64,
    kept: Option<u64>,
) -> Result<Listed, Error> {
    let (openpgp, openpgp_failed) = arrived(openpgp);
    let (ed25519, ed25519_failed) = arrived(ed25519);
    let signatures = Signatures {
        openpgp: openpgp.as_deref(),
        ed25519: ed25519.as_deref(),
    };
    match (
        listed(bytes, signatures, now, kept),
        openpgp_failed.or(ed25519_failed),
    ) {
        (Err(Error::Refused(directory::Error::Unsigned)), Some(failed)) => Err(failed),
        (result, _) => result,
    }
}

/// A download's signature, if one arrived, and its failure, if it failed.
fn arrived(download: Result<Option<Vec<u8>>, Error>) -> (Option<Vec<u8>>, Option<Error>) {
    match download {
        Ok(signature) => (signature, None),
        Err(error) => (None, Some(error)),
    }
}

/// What a downloaded list offers at `now` (Unix seconds), once checked,
/// refused when its serial is below `kept`, the newest taken before.
pub(crate) fn listed(
    bytes: &[u8],
    signatures: Signatures<'_>,
    now: u64,
    kept: Option<u64>,
) -> Result<Listed, Error> {
    Ok(Directory::verify(bytes, signatures, now, kept)?.into())
}

impl From<signed::Signed> for Listed {
    fn from(signed: signed::Signed) -> Self {
        Self {
            expires: signed.expires,
            serial: signed.serial,
            relays: signed
                .directory
                .relays
                .into_iter()
                .map(|relay| Public {
                    address: relay.endpoint.to_string(),
                    name: relay.name,
                    region: relay.region,
                    quic_pin: relay.quic_pin,
                })
                .collect(),
            signer: signed.signer,
        }
    }
}

/// The file at `url`, at most `limit` bytes of it, or `None` when the site
/// has none: a signature either signer may leave out.
fn download(agent: &ureq::Agent, url: &str, limit: usize) -> Result<Option<Vec<u8>>, Error> {
    let mut response = match agent.get(url).call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(source) => return Err(Error::Download(source)),
    };
    // One byte past the limit, so a longer file is refused, not cut short.
    let bytes = response
        .body_mut()
        .with_config()
        .limit(limit as u64 + 1)
        .read_to_vec()
        .map_err(Error::Download)?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests;
