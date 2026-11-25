use super::*;
use sheprd_remote_control::control::{
    Device,
    hosts::{Enrolled, HostLink},
};

fn to(host: &str, device: &str) -> HostDevice {
    HostDevice {
        host: HostId(host.to_owned()),
        device: DeviceId(device.to_owned()),
    }
}

fn nears(run: &Run) -> Vec<(&str, bool)> {
    let mut nears: Vec<_> = run
        .marks
        .iter()
        .map(|(device, mark)| (device.0.as_str(), mark.near))
        .collect();
    nears.sort_unstable();
    nears
}

#[test]
fn a_mark_goes_once_per_host_with_the_newest_per_device() {
    let mut marks = HostMarks::default();
    let now = Instant::now();
    marks.mark(to("ssh:a", "pa"), true, now);
    marks.mark(to("ssh:a", "pa"), false, now);
    marks.mark(to("ssh:b", "pb"), true, now);
    let mut runs = marks.take(now);
    runs.sort_by(|a, b| a.host.cmp(&b.host));
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].host, HostId("ssh:a".to_owned()));
    assert_eq!(nears(&runs[0]), [("pa", false)]);
    assert_eq!(nears(&runs[1]), [("pb", true)]);
    // Nothing more is owed.
    assert!(marks.take(now).is_empty());
}

#[test]
fn a_host_runs_one_at_a_time_and_keeps_what_came_meanwhile() {
    let mut marks = HostMarks::default();
    let now = Instant::now();
    marks.mark(to("ssh:a", "p"), true, now);
    let run = marks.take(now).pop().unwrap();
    marks.mark(to("ssh:a", "p"), false, now);
    assert!(marks.take(now).is_empty(), "the first run is still out");
    marks.finished(run, Outcome::Sent, now);
    let next = marks.take(now).pop().unwrap();
    assert_eq!(nears(&next), [("p", false)]);
}

#[test]
fn a_failed_run_is_retried_later_unless_a_newer_mark_came() {
    let mut marks = HostMarks::default();
    let now = Instant::now();
    marks.mark(to("ssh:a", "p"), true, now);
    marks.mark(to("ssh:a", "q"), true, now);
    let run = marks.take(now).pop().unwrap();
    marks.mark(to("ssh:a", "q"), false, now);
    marks.finished(run, Outcome::Failed, now);
    assert!(marks.take(now).is_empty(), "rests after a failure");
    let later = now + RETRY;
    let retry = marks.take(later).pop().unwrap();
    assert_eq!(nears(&retry), [("p", true), ("q", false)]);
}

#[test]
fn marks_too_old_to_matter_are_dropped() {
    let mut marks = HostMarks::default();
    let now = Instant::now();
    marks.mark(to("ssh:a", "p"), false, now);
    let run = marks.take(now).pop().unwrap();
    marks.finished(run, Outcome::Failed, now);
    // The host let any near mark lapse by now.
    assert!(marks.take(now + LAPSE).is_empty());
    assert!(marks.hosts.is_empty(), "nothing is kept for the host");
}

#[test]
fn a_host_no_window_connects_to_is_forgotten() {
    let mut marks = HostMarks::default();
    let now = Instant::now();
    marks.mark(to("ssh:a", "p"), true, now);
    let run = marks.take(now).pop().unwrap();
    marks.mark(to("ssh:a", "p"), false, now);
    marks.finished(run, Outcome::Gone, now);
    assert!(marks.take(now).is_empty());
}

fn device(id: &str, hosts: Vec<HostLink>) -> Device {
    Device {
        id: DeviceId(id.to_owned()),
        name: "iPhone".to_owned(),
        created_ms: 0,
        last_seen_ms: None,
        push: true,
        connected: false,
        share_github: true,
        via: None,
        hosts,
    }
}

fn link(host: &str, state: HostState) -> HostLink {
    HostLink {
        host: HostId(host.to_owned()),
        name: host.to_owned(),
        state,
    }
}

#[test]
fn only_hosts_a_phone_was_paired_with_are_marked() {
    let enrolled = Enrolled {
        device: DeviceId("there".to_owned()),
        computer: "box".to_owned(),
        fingerprint: "aa".to_owned(),
        origins: Vec::new(),
    };
    let answer = Ok(Response::Devices {
        devices: vec![
            device(
                "here",
                vec![
                    link("ssh:a", HostState::Paired(enrolled)),
                    link("ssh:b", HostState::Allowed),
                    link("ssh:c", HostState::Offered),
                ],
            ),
            device("other", Vec::new()),
        ],
    });
    let paired = paired(answer);
    assert_eq!(
        paired.get(&DeviceId("here".to_owned())),
        Some(&vec![to("ssh:a", "there")])
    );
    assert_eq!(paired.get(&DeviceId("other".to_owned())), Some(&Vec::new()));
    assert!(super::paired(Err(control::Error::NoAnswer)).is_empty());
}

#[test]
fn the_script_marks_each_device_quoted_and_fails_when_one_failed() {
    let now = Instant::now();
    let marks = HashMap::from([
        (
            DeviceId("a b".to_owned()),
            Mark {
                near: true,
                at: now,
            },
        ),
        (
            DeviceId("c".to_owned()),
            Mark {
                near: false,
                at: now,
            },
        ),
    ]);
    let script = script(&marks);
    assert!(script.contains("presence --device 'a b' --near || status=1"));
    assert!(script.contains("presence --device 'c' --away || status=1"));
    assert!(script.ends_with("fi\nexit $status\n"));
}
