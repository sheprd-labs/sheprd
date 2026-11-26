#![allow(clippy::unwrap_used)]
use super::Outcome;
use crate::sidebar::layout_tests::{fixture_window, full_draw};
use gpui::{TestAppContext, px, size};
use sheprd_client::ConnectTarget;

const HOST: &str = "ssh:0123456789abcdef0123456789abcdef";
const WSL: &str = "wsl:Ubuntu";

fn add_devices(view: &mut crate::SheprdWindow, enabled: bool) {
    view.endpoints[0].connection.target = ConnectTarget::Local;
    view.endpoints[0].collapsed = true;
    view.endpoints.push(crate::endpoint::Endpoint::new(
        HOST.into(),
        "studio".into(),
        ConnectTarget::Ssh {
            target: "penso@studio".into(),
            session: "default".into(),
        },
        enabled,
    ));
    view.endpoints.push(crate::endpoint::Endpoint::new(
        WSL.into(),
        "Ubuntu".into(),
        ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        },
        true,
    ));
}

#[gpui::test]
fn only_saved_ssh_and_cloud_devices_pause(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        add_devices(&mut view, true);
        view
    });
    view.update(cx, |view, cx| {
        assert!(view.device_pausable(HOST));
        assert!(!view.device_pausable(crate::endpoint::LOCAL));
        assert!(!view.device_pausable(WSL));
        assert!(!view.device_pausable("ssh:missing"));
        assert!(!view.device_paused(HOST));
        // Nothing runs for a device that cannot pause.
        assert!(view.set_device_paused(WSL, true, cx).is_none());
        assert!(
            view.set_device_paused(crate::endpoint::LOCAL, true, cx)
                .is_none()
        );
        // A development catalog is not the one `herdr machine` edits.
        view.endpoints[0].connection.target = ConnectTarget::Session {
            name: "default".into(),
            development: true,
        };
        assert!(!view.device_pausable(HOST));
    });
}

#[gpui::test]
fn a_paused_device_stays_listed_and_says_so(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        add_devices(&mut view, false);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| assert!(view.device_paused(HOST)));
    assert!(
        cx.debug_bounds("host-ssh:0123456789abcdef0123456789abcdef")
            .is_some()
    );
    assert!(
        cx.debug_bounds("host-working-ssh:0123456789abcdef0123456789abcdef")
            .is_some()
    );
    // A running device has no such word in the Workspaces layout.
    view.update(cx, |view, _| view.endpoints[1].enabled = true);
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(
        cx.debug_bounds("host-working-ssh:0123456789abcdef0123456789abcdef")
            .is_none()
    );
}

// The writes are finished by hand: the background ones never run, as the
// test never parks.
#[gpui::test]
fn a_resume_right_after_a_pause_is_written_after_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        add_devices(&mut view, true);
        view
    });
    view.update(cx, |view, cx| {
        let pause = view.set_device_paused(HOST, true, cx).unwrap();
        assert!(view.device_paused(HOST));
        let resume = view.set_device_paused(HOST, false, cx).unwrap();
        // The latest request shows at once, before anything is saved.
        assert!(!view.device_paused(HOST));
        assert_eq!(view.pause_outcome(HOST, pause), None);
        let pending = &view.menu.pauses.devices[HOST];
        assert_eq!(pending.writing, Some(pause));
        // The pause lands, then the resume is written after it.
        view.pause_written(HOST, Some(pause), Ok(()), cx);
        assert_eq!(view.menu.pauses.devices[HOST].writing, Some(resume));
        assert_eq!(view.pause_outcome(HOST, pause), Some(Outcome::Saved));
        assert_eq!(view.pause_outcome(HOST, resume), None);
        view.pause_written(HOST, Some(resume), Ok(()), cx);
        assert_eq!(view.pause_outcome(HOST, resume), Some(Outcome::Saved));
        assert!(!view.device_paused(HOST));
        // The catalog shows the device running, so the request is forgotten.
        view.prune_device_pauses();
        assert!(view.menu.pauses.devices.is_empty());
    });
}

#[gpui::test]
fn a_failed_pause_reports_its_error_and_the_device_reads_as_saved(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        add_devices(&mut view, true);
        view
    });
    view.update(cx, |view, cx| {
        let pause = view.set_device_paused(HOST, true, cx).unwrap();
        view.pause_written(HOST, Some(pause), Err("profile not found".into()), cx);
        assert_eq!(
            view.pause_outcome(HOST, pause),
            Some(Outcome::Failed("profile not found".into()))
        );
        assert!(!view.device_paused(HOST));
        // Kept a while, so a waiting `host pause` still reads the error.
        view.prune_device_pauses();
        assert!(view.menu.pauses.devices.contains_key(HOST));
    });
}

#[gpui::test]
fn a_paused_device_names_no_core_target(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        add_devices(&mut view, true);
        view
    });
    view.update(cx, |view, _| {
        let scope = crate::browser::scope(&view.endpoints[1]);
        assert!(view.saved_target(&scope).is_some());
        view.endpoints[1].enabled = false;
        assert_eq!(view.saved_target(&scope), None);
    });
}

// Cloud devices keep the app's own list, which development sessions share.
#[cfg(feature = "cloud")]
#[gpui::test]
fn a_cloud_device_pauses_in_a_development_session(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        add_devices(&mut view, true);
        view.endpoints[0].connection.target = ConnectTarget::Session {
            name: "default".into(),
            development: true,
        };
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "cloud:w1".into(),
            "Build box".into(),
            ConnectTarget::Cloud {
                provider: crate::cloud::CloudProvider::ALL[0],
                account: "https://cloud.example.com".into(),
                id: "w1".into(),
                machine: "herdr-tidy-lark-aa".into(),
                session: "default".into(),
            },
            true,
        ));
        view
    });
    view.read_with(cx, |view, _| {
        assert!(view.device_pausable("cloud:w1"));
        assert!(!view.device_pausable(HOST));
    });
}
