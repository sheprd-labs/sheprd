use super::*;
use crate::limits::{LIVENESS_TIMEOUT, PING_TIMEOUT};

#[test]
fn a_liveness_probe_pings_at_once_and_fails_sooner_than_a_routine_ping() {
    let now = Instant::now();
    let mut health = Health::new(now);
    assert!(health.probe(now, LIVENESS_TIMEOUT));
    assert!(!health.tick(now + LIVENESS_TIMEOUT - POLL).unwrap());
    assert!(matches!(
        health.tick(now + LIVENESS_TIMEOUT),
        Err(Error::HealthTimeout)
    ));

    // A probe while a routine ping is outstanding only brings its deadline forward.
    let mut health = Health::new(now);
    assert!(health.tick(now + Duration::from_secs(5)).unwrap());
    let probed = now + Duration::from_secs(6);
    assert!(!health.probe(probed, LIVENESS_TIMEOUT));
    assert!(health.tick(probed + LIVENESS_TIMEOUT).is_err());

    // An answer clears it, and a later probe sends a new ping.
    let mut health = Health::new(now);
    assert!(health.probe(now, LIVENESS_TIMEOUT));
    health.received(now + POLL);
    assert!(!health.tick(now + LIVENESS_TIMEOUT).unwrap());
    assert!(health.probe(now + LIVENESS_TIMEOUT, LIVENESS_TIMEOUT));
}

/// An SSH-mode client whose link has sent its welcome, with health checks,
/// and its first snapshot.
fn connected_ssh_link() -> (Client, Stream, thread::JoinHandle<Result<()>>) {
    let (client, mut server, worker) = test_client_mode(false, true);
    server
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    receive(&mut server);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!([
        "surface_interest",
        "presentation_effects_fence",
        "health_check"
    ]);
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    event(&client);
    event(&client);
    (client, server, worker)
}

#[test]
fn check_liveness_sends_a_ping_now_on_an_ssh_link_and_drops_a_silent_one() {
    let (client, mut server, worker) = connected_ssh_link();
    let asked = Instant::now();
    client.handle.check_liveness(Suspicion::Slept);
    assert!(
        matches!(receive(&mut server), ClientMessage::EndpointControl { kind, .. } if kind == "endpoint.health.ping.v1")
    );
    // Well before a quiet link's five seconds: the probe sent it.
    assert!(asked.elapsed() < Duration::from_secs(2));
    // Unanswered, the link fails on the probe's deadline, not the routine one.
    assert!(matches!(worker.join().unwrap(), Err(Error::HealthTimeout)));
    let waited = asked.elapsed();
    assert!(
        waited >= LIVENESS_TIMEOUT && waited < PING_TIMEOUT,
        "{waited:?}"
    );
}

#[test]
fn the_strongest_suspicion_not_yet_sent_sets_the_deadline() {
    let pace = Pace::default();
    assert_eq!(Suspicion::Slept.timeout(pace), LIVENESS_TIMEOUT);
    assert_eq!(Suspicion::NetworkChanged.timeout(pace), PING_TIMEOUT);
    let request = crate::liveness::Request::default();
    assert!(!request.pending());
    request.ask(Suspicion::Slept);
    request.ask(Suspicion::NetworkChanged);
    assert!(request.pending());
    assert_eq!(request.take(), Some(Suspicion::Slept));
    assert_eq!(request.take(), None);
    request.ask(Suspicion::NetworkChanged);
    assert_eq!(request.take(), Some(Suspicion::NetworkChanged));
}

/// A network change pings at once, but a link still catching up after a
/// short stall may answer later than a sleep's probe allows and stay up.
#[test]
fn a_network_change_probe_lets_a_slow_answer_keep_the_link() {
    let (client, mut server, worker) = connected_ssh_link();
    let asked = Instant::now();
    client.handle.check_liveness(Suspicion::NetworkChanged);
    assert!(
        matches!(receive(&mut server), ClientMessage::EndpointControl { kind, .. } if kind == "endpoint.health.ping.v1")
    );
    assert!(asked.elapsed() < Duration::from_secs(2));
    thread::sleep(
        (asked + LIVENESS_TIMEOUT + Duration::from_secs(1))
            .saturating_duration_since(Instant::now()),
    );
    assert!(!worker.is_finished(), "{:?}", asked.elapsed());
    // Any complete message answers the probe.
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    event(&client);
    client.handle.disconnect();
    assert!(worker.join().unwrap().is_ok());
}
