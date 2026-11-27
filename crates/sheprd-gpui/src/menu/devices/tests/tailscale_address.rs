//! The address a Tailscale SSH check names, kept with the refusal that named it.
//!
//! The dialog renders that address as a link, and reads it only from the stored
//! failure — never out of the message text, where it is prose nothing can click.
//! So a refusal that reaches the dialog without its failure shows an address the
//! user can read and cannot open, which is the whole complaint these tests pin.

use super::*;
use sheprd_client::{SshFailure, TailscaleCheck};

const ADDRESS: &str = "https://login.tailscale.com/a/o84652-335960037";

fn refused(
    name: &str,
    failure: SshFailure,
) -> crate::Result<(setup::Claim, crate::Result<HostProbe>)> {
    Ok((
        setup::Claim::fixture(name),
        Ok(HostProbe::SshFailed(failure)),
    ))
}

fn tailscale(name: &str) -> crate::Result<(setup::Claim, crate::Result<HostProbe>)> {
    refused(
        name,
        SshFailure::Tailscale(TailscaleCheck::from_address(ADDRESS).unwrap()),
    )
}

/// A check Tailscale refused is shown with the address that answers it, so the
/// user has something to act on rather than a sentence to copy out.
#[gpui::test]
fn a_refused_check_is_stored_with_its_address(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("user@100.106.27.154", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(tailscale("tailscale-refused"), cx);

            let form = view.menu.device_setup.as_ref().unwrap();
            // A terminal cannot finish this check, so none is offered for it.
            assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
            assert_eq!(
                crate::tailscale::pending(form.failure()).and_then(TailscaleCheck::url),
                Some(ADDRESS),
                "the dialog must still hold the address to link to"
            );
        });
    });
}

/// Any other refusal says what SSH refused and shows no link: one beside a
/// message that names no address would open a check nobody was asked to do.
#[gpui::test]
fn another_refusal_offers_no_address(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("user@100.106.27.155", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(refused("ssh-unreachable", SshFailure::Unreachable), cx);

            let form = view.menu.device_setup.as_ref().unwrap();
            assert!(form.error().is_some(), "the refusal is still said");
            assert_eq!(form.failure(), Some(&SshFailure::Unreachable));
            assert!(crate::tailscale::pending(form.failure()).is_none());
        });
    });
}

/// Clearing the message clears the failure with it, so an address can never
/// outlive the refusal it belongs to.
#[gpui::test]
fn a_cleared_refusal_takes_its_address_with_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("user@100.106.27.157", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(tailscale("tailscale-clear"), cx);
            assert!(view.menu.device_setup.as_ref().unwrap().failure().is_some());

            view.set_device_error(None);
            let form = view.menu.device_setup.as_ref().unwrap();
            assert!(form.error().is_none());
            assert!(
                form.failure().is_none(),
                "an address outliving its message would outlive the check it answers"
            );
        });
    });
}
