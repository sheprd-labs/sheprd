use std::{fs, time::Duration};

use crate::sharing::{
    Content, Error, History, Source, Store,
    tests::{Fixture, clock},
};

#[test]
fn restart_cannot_revive_revoked_or_previously_live_grants() {
    let mut f = Fixture::new();
    let (a, _) = f.share(History::All, Content::default());
    let (b, _) = f.share(History::All, Content::default());
    f.store.revoke(a.share, a.grant, clock(0)).unwrap();
    let path = f.dir.path().join("sharing");
    drop(f.store);
    let mut store = Store::open(&path, clock(0)).unwrap();
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    for invite in [a, b] {
        assert!(matches!(
            store.admit(
                invite.share,
                invite.grant,
                &invite.capability,
                source,
                clock(0)
            ),
            Err(Error::Denied)
        ));
    }
    assert!(
        store
            .create(
                source,
                History::FromNow,
                Content::default(),
                Duration::from_secs(60),
                clock(0)
            )
            .is_ok()
    );
}

#[test]
fn another_writer_cannot_open_the_same_store() {
    let f = Fixture::new();
    assert!(matches!(
        Store::open(&f.dir.path().join("sharing"), clock(0)),
        Err(Error::Busy)
    ));
}

#[test]
fn state_has_only_capability_verifiers() {
    let mut f = Fixture::new();
    let (invite, _) = f.share(History::All, Content::default());
    let bytes = fs::read(f.dir.path().join("sharing/shares.json")).unwrap();
    let state: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let grant = &state["shares"][0]["grants"][0];
    let verifier: Vec<u8> = serde_json::from_value(grant["digest"].clone()).unwrap();
    assert_ne!(verifier, invite.capability.expose());
    assert!(grant.get("capability").is_none());
}

#[test]
fn unknown_corrupt_oversized_and_semantically_invalid_state_fail_closed() {
    for data in [
        br#"{"version":900,"shares":[]}"#.to_vec(),
        br#"{"version":1,"shares":[],"unexpected":true}"#.to_vec(),
        b"truncated state".to_vec(),
        vec![b' '; 256 * 1024 + 1],
    ] {
        let f = Fixture::new();
        let path = f.dir.path().join("sharing");
        drop(f.store);
        fs::write(path.join("shares.json"), &data).unwrap();
        assert!(Store::open(&path, clock(0)).is_err());
        assert_eq!(fs::read(path.join("shares.json")).unwrap(), data);
    }
    let mut f = Fixture::new();
    f.share(History::All, Content::default());
    let path = f.dir.path().join("sharing");
    drop(f.store);
    let mut state: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("shares.json")).unwrap()).unwrap();
    state["shares"][0]["expires_ms"] = 1.into();
    fs::write(
        path.join("shares.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        Store::open(&path, clock(0)),
        Err(Error::InvalidState)
    ));
}

#[test]
fn a_failed_revocation_write_closes_all_live_access_and_restart_stays_closed() {
    let mut f = Fixture::new();
    let (invite, guest) = f.share(History::All, Content::default());
    let path = f.dir.path().join("sharing/shares.json");
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        f.store
            .revoke(invite.share, invite.grant, clock(1))
            .is_err()
    );
    assert!(matches!(f.read(&guest, None, 1), Err(Error::Disabled)));
    fs::remove_dir(&path).unwrap();
    sheprd_client::private_file::prepare(&path).unwrap();
    fs::write(&path, saved).unwrap();
    drop(f.store);
    let mut store = Store::open(path.parent().unwrap(), clock(0)).unwrap();
    let source = Source {
        endpoint: &f.endpoint,
        cursor: f.cursor,
        transcript: &f.transcript,
    };
    assert!(matches!(
        store.admit(
            invite.share,
            invite.grant,
            &invite.capability,
            source,
            clock(0)
        ),
        Err(Error::Denied)
    ));
}

#[cfg(unix)]
#[test]
fn state_is_private_and_symlink_substitution_is_refused() {
    use std::os::unix::fs::{MetadataExt, symlink};
    let mut f = Fixture::new();
    let (invite, guest) = f.share(History::All, Content::default());
    let path = f.dir.path().join("sharing/shares.json");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    let target = f.dir.path().join("untouched");
    fs::write(&target, b"sentinel").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&target, &path).unwrap();
    assert!(matches!(
        f.store.revoke(invite.share, invite.grant, clock(0)),
        Err(Error::Private(_))
    ));
    assert_eq!(fs::read(target).unwrap(), b"sentinel");
    assert!(matches!(f.read(&guest, None, 0), Err(Error::Disabled)));
}
