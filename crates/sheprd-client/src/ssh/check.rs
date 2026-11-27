//! The Tailscale SSH identity check that holds a connection waiting.
//!
//! Tailscale SSH answers authentication from the tailnet policy instead of the
//! client's keys, and when that policy asks for proof of identity it puts the
//! connection in check mode: the server sends an auth banner naming an address
//! the user has to open in a browser, then holds the connection until they do
//! (`SendAuthBanner` and the `HoldAndDelegate` action in Tailscale's
//! `ipn/ipnlocal`). OpenSSH prints that banner to stderr at its default
//! `LogLevel` and never prompts, so a client run with `BatchMode=yes` — which
//! is how Sheprd runs every SSH probe, see [`super::POLICY`] — waits out its
//! whole life reporting nothing but a timeout. Recognising the banner is what
//! turns "SSH discovery timed out" into a question the user can answer.
//!
//! The address authorises the one connection that asked for it, so it is a
//! capability rather than a diagnostic: [`TailscaleCheck`]'s `Debug` redacts
//! it and only [`TailscaleCheck::url`] hands it over, to the person being
//! asked to authenticate.

use std::fmt;

/// The sentence Tailscale's banner opens with, before the address.
const MARKER: &str = "Tailscale SSH requires an additional check";

/// The origin of the address Tailscale tells the user to open. A link that
/// names anywhere else is not Tailscale asking, whatever a banner claims.
const ADDRESS: &str = "https://login.tailscale.com/a/";

/// Tailscale's tokens are far shorter than this; it only bounds what a banner
/// can make the client hold, so no host can size a link Sheprd shows.
const TOKEN_LIMIT: usize = 128;

/// A connection waiting for the user to prove their identity to Tailscale.
///
/// `url` is the address `ssh` was told to open, when its output carried a
/// well-formed one. `None` is a banner Sheprd recognised but could not read,
/// which still says why the connection is waiting.
#[derive(Clone, PartialEq, Eq)]
pub struct TailscaleCheck {
    url: Option<String>,
}

impl TailscaleCheck {
    /// The address to open to authenticate, when the banner carried one. Only
    /// the user being asked should be shown it; see the module docs.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// A check for an address that came from somewhere other than a banner,
    /// when [`address`] proves it is one of Tailscale's own sign-in addresses.
    /// This is the only way to build a check from an address alone, so no caller
    /// can hand the UI a clickable link that text the host chose invented.
    pub fn from_address(url: &str) -> Option<Self> {
        address(url).map(|url| Self {
            url: Some(url.to_owned()),
        })
    }
}

/// What to tell a user who is not being handed the address: a check belongs to
/// the connection that asked, so the way to finish one Sheprd has given up on
/// is to run `ssh` and complete the check that connection prints.
impl fmt::Display for TailscaleCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.url {
            Some(url) => write!(f, "open {url} to authenticate, then try again"),
            None => write!(
                f,
                "connect to this host from a terminal and complete the check it asks for"
            ),
        }
    }
}

impl fmt::Debug for TailscaleCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Redacted: whoever opens the address authenticates this connection.
        f.debug_struct("TailscaleCheck")
            .field(
                "url",
                &self.url.as_ref().map(|_| "<authentication address>"),
            )
            .finish()
    }
}

/// The check pending in what `ssh` wrote to stderr. Its banner is sent before
/// any key is offered or a host key is even checked, so this is decided ahead
/// of the ordinary failures.
pub(crate) fn pending(stderr: &[u8]) -> Option<TailscaleCheck> {
    let stderr = String::from_utf8_lossy(stderr);
    let url = address(&stderr);
    // Either half identifies the banner: the wording moves between Tailscale
    // versions, and so could the sentence around the address.
    (url.is_some() || stderr.contains(MARKER)).then_some(TailscaleCheck { url })
}

/// The check pending in output that may still be arriving.
///
/// A banner is written over several reads, so a tail that is still growing can
/// hold the marker with the address not yet written, or an address that runs to
/// the end of what has arrived and may still be growing. Neither is reported:
/// this waits for the whole address, and [`pending`] is what reads output that
/// has finished.
pub(crate) fn pending_live(stderr: &[u8]) -> Option<TailscaleCheck> {
    let stderr = String::from_utf8_lossy(stderr);
    let (url, ends) = address_ending(&stderr)?;
    // An address that stops before the end of the text is followed by a
    // boundary that actually arrived; one at the very end may still grow.
    (ends < stderr.len()).then_some(TailscaleCheck { url: Some(url) })
}

/// The sign-in address `text` names, when it is one of Tailscale's own and
/// ends where Sheprd can prove it ends.
///
/// Both ends are checked, because a banner is remote text and an address that
/// is only part of a larger one would be a link to somewhere else:
///
/// - it must not sit inside another URL. `?next=https://login.tailscale.com/a/…`
///   is somebody else's page that mentions Tailscale, and opening it is not
///   authenticating with Tailscale;
/// - the token must stop at a boundary. `…/a/bad.token` and `…/a/abc@host` are
///   not tokens Sheprd can read, and handing back the truncated `bad` or `abc`
///   would be a plausible address that authorises nothing.
///
/// An address that fails either test is dropped and the search continues, so
/// one unreadable mention does not hide a real one after it.
fn address(text: &str) -> Option<String> {
    address_ending(text).map(|(url, _)| url)
}

/// [`address`], also returning the index just past the address, so a caller
/// reading output that is still arriving can tell an address with a boundary
/// after it from one that runs to the end of what has been read.
fn address_ending(text: &str) -> Option<(String, usize)> {
    let mut from = 0;
    while let Some(at) = text[from..].find(ADDRESS) {
        let start = from + at;
        let after = start + ADDRESS.len();
        let token: String = text[after..]
            .chars()
            .take_while(|c| matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'))
            .collect();
        let ends = after + token.len();
        let readable = text[..start].chars().next_back().is_none_or(boundary)
            && text[ends..].chars().next().is_none_or(boundary);
        if !token.is_empty() && token.len() <= TOKEN_LIMIT && readable {
            return Some((format!("{ADDRESS}{token}"), ends));
        }
        from = after;
    }
    None
}

/// What an address may stand next to in a banner: whitespace or a closing mark.
/// A dot is deliberately not one, since domains and paths are full of them, so
/// anything that could continue a URL — or is not ASCII at all — makes the
/// address unreadable rather than truncated.
fn boundary(c: char) -> bool {
    c.is_ascii_whitespace() || matches!(c, ',' | ';' | ')' | '\'' | '"' | ']' | '>')
}

#[cfg(test)]
mod tests;
