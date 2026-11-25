//! A nearby phone or computer that agreed on a code over Bluetooth: the
//! dialog shows it at once, before the claim brings Allow.

use super::*;

fn nearby_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
            nearby_code: Some("906492".into()),
        }),
        _ => Response::Done,
    })
}

#[gpui::test]
fn the_code_shows_as_soon_as_a_nearby_device_has_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, nearby_daemon);
    open(&view, cx);
    cx.run_until_parked();
    wait(cx);
    assert!(matches!(
        phase(&view, cx),
        Phase::Showing { nearby_code: Some(code), .. } if code == "906492"
    ));
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-nearby-code").is_some());
    assert!(cx.debug_bounds("remote-pair-code").is_some());
    // The QR gives way: someone else scanning now would only race the
    // device already pairing.
    assert!(cx.debug_bounds("remote-pair-qr").is_none());
}
