//! The Tailscale SSH identity check, wherever the app says it is waiting.
//!
//! A host that authenticates over Tailscale holds the connection open until the
//! user proves their identity in a browser, and names the address to prove it at
//! in `ssh`'s banner. That banner is remote text, so nothing here opens on its
//! own: what these surfaces paint, and what only a click hands to a browser, is
//! the address `sheprd-client` proved to be one of Tailscale's own sign-in
//! addresses.
//!
//! Every surface reads the same state through [`check`] — the typed failure when
//! the transport filled it, the sentence a refusal left otherwise — so the
//! status bar, the prompt, and the dialog cannot disagree about whether a check
//! is pending.

use crate::config::Theme;
use gpui::{prelude::*, *};
use sheprd_client::{SshFailure, TailscaleCheck};

/// The check a connection is waiting on: the typed failure when the transport
/// filled it, otherwise the one a sentence names.
///
/// A refusal can reach the window as reason text rather than as
/// [`SshFailure::Tailscale`] — the transport reports `PermissionDenied` and the
/// sentence carries the address — so the text is read for one too. Only an
/// address Tailscale's own validator accepts becomes a check, so a host cannot
/// turn its own text into a link.
pub(crate) fn check(
    failure: Option<&SshFailure>,
    sentence: Option<&str>,
) -> Option<TailscaleCheck> {
    if let Some(check) = pending(failure) {
        return Some(check.clone());
    }
    let url = first_address(sentence?)?;
    TailscaleCheck::from_address(url)
}

/// The check a typed failure names, when it carries an address to open.
pub(crate) fn pending(failure: Option<&SshFailure>) -> Option<&TailscaleCheck> {
    match failure? {
        SshFailure::Tailscale(check) if check.url().is_some() => Some(check),
        _ => None,
    }
}

/// The first sign-in address a sentence names, read with Tailscale's validator
/// so that only a link Sheprd could prove belongs to Tailscale becomes one.
fn first_address(text: &str) -> Option<&str> {
    text.split_whitespace()
        .map(|word| word.trim_end_matches(['.', ',', ';', ')', ':', '"', '\'']))
        .find(|word| TailscaleCheck::from_address(word).is_some())
}

/// Hand an address to the browser, after copying it. GPUI passes the URI to the
/// compositor through an async `xdg-activation` token, and a token that never
/// arrives opens nothing with no feedback, so the copy keeps the click useful.
/// One definition, so no surface can hand an address over without it.
pub(crate) fn open_and_copy(url: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(url.to_owned()));
    cx.open_url(url);
}

/// The address on its own line, clickable, for a surface with room for it. The
/// whole address is shown, so a person can read what they are opening before
/// they open it.
pub(crate) fn address_row(theme: &Theme, url: &str) -> Stateful<Div> {
    let open = url.to_owned();
    div()
        .id("tailscale-address")
        .debug_selector(|| "tailscale-address".into())
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(|row| row.underline())
        .on_click(move |_, _, cx| open_and_copy(&open, cx))
        .child(url.to_owned())
}

#[cfg(test)]
mod tests;
