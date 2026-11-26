use super::*;
use std::ffi::OsStr;

fn after_session(command: &Command) -> Vec<&OsStr> {
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(args[0], crate::setup_relay::SESSION);
    args[1..].to_vec()
}

#[test]
fn setup_prepares_path_then_runs_machine_add_without_interpolating_arguments() -> Result<()> {
    let request = Request::new("user@host", "Alice's $(printf INJECTED); device", "work")?;
    let command = setup_command(
        OsStr::new("/app/sheprd"),
        OsStr::new("/a path/herdr"),
        &request,
    );
    assert_eq!(command.get_program(), "/app/sheprd");
    let words = after_session(&command);
    assert_eq!(words[0], "/bin/sh");
    assert_eq!(words[1], "-c");
    assert_eq!(words[4], request.target());
    assert_eq!(words[7], "/a path/herdr");
    let arguments: Vec<_> = request.arguments().into_iter().map(OsStr::new).collect();
    assert_eq!(&words[8..], arguments);
    assert!(words[5].to_string_lossy().contains(".zshenv"));
    Ok(())
}

#[test]
fn key_setup_runs_only_ssh_copy_id_without_herdr_1() -> Result<()> {
    let request = Request::new("user@host", "Box", "")?;
    let command = key_setup_command(OsStr::new("/app/sheprd"), &request);
    assert_eq!(command.get_program(), "/app/sheprd");
    let words = after_session(&command);
    assert_eq!(
        words,
        [
            OsStr::new("ssh-copy-id"),
            OsStr::new("--"),
            OsStr::new("user@host")
        ]
    );

    let unverified = request.with_host_keys(HostKeys::Skip);
    let command = key_setup_command(OsStr::new("/app/sheprd"), &unverified);
    let words = after_session(&command);
    assert_eq!(words[0], "ssh-copy-id");
    assert!(words.contains(&OsStr::new("StrictHostKeyChecking=no")));
    assert_eq!(
        words[words.len() - 2..],
        [OsStr::new("--"), OsStr::new("user@host")]
    );
    Ok(())
}

#[test]
fn install_core_installs_configures_path_and_stores_without_herdr_1() -> Result<()> {
    use std::cell::RefCell;
    let request = Request::new("penso@box", "Box", "")?;
    let claim = Claim::acquire(destination("core-install.test"), "default")?;
    let cancelled = AtomicBool::new(false);
    let steps = RefCell::new(Vec::new());
    install_core_with(
        &request,
        &claim,
        &cancelled,
        |_, _| {
            steps.borrow_mut().push("verify");
            Ok(())
        },
        |_| {
            steps.borrow_mut().push("keys");
            Ok(())
        },
        |target, _| {
            assert_eq!(target, "penso@box");
            steps.borrow_mut().push("install");
            Ok(())
        },
        |target, _| {
            assert_eq!(target, "penso@box");
            steps.borrow_mut().push("path");
        },
        |_, _| {
            steps.borrow_mut().push("store");
            Ok(())
        },
    )?;
    assert_eq!(
        *steps.borrow(),
        ["verify", "keys", "install", "path", "store"]
    );

    // An install failure stops before configuring PATH or saving the host.
    let failed_steps = RefCell::new(Vec::new());
    let Err(error) = install_core_with(
        &request,
        &claim,
        &cancelled,
        |_, _| Ok(()),
        |_| Ok(()),
        |_, _| {
            Err(crate::Error::Remote(
                sheprd_remote::core_install::Error::NoCurl.into(),
            ))
        },
        |_, _| failed_steps.borrow_mut().push("path"),
        |_, _| {
            failed_steps.borrow_mut().push("store");
            Ok(())
        },
    ) else {
        panic!("expected core install failure");
    };
    assert!(matches!(
        error,
        crate::Error::Remote(sheprd_remote::Error::CoreInstall(
            sheprd_remote::core_install::Error::NoCurl
        ))
    ));
    assert!(failed_steps.borrow().is_empty());

    // Cancelling during PATH configuration must abort without saving the device.
    let cancel_during_path = AtomicBool::new(false);
    let stored = RefCell::new(false);
    let Err(error) = install_core_with(
        &request,
        &claim,
        &cancel_during_path,
        |_, _| Ok(()),
        |_| Ok(()),
        |_, _| Ok(()),
        |_, cancelled| cancelled.store(true, Ordering::Release),
        |_, _| {
            *stored.borrow_mut() = true;
            Ok(())
        },
    ) else {
        panic!("expected cancellation error");
    };
    assert!(matches!(
        error,
        crate::Error::Client(sheprd_client::Error::ScriptCancelled)
    ));
    assert!(!*stored.borrow());
    Ok(())
}

#[test]
fn path_prepare_must_succeed_before_the_device_is_saved() -> Result<()> {
    let root = tempfile::tempdir()?;
    let log = root.path().join("log");
    for (name, script) in [
        (
            "ssh",
            "#!/bin/sh\nprintf 'path\\n' >> \"$LOG\"\nexit \"$SSH_EXIT\"\n",
        ),
        ("herdr", "#!/bin/sh\nprintf 'save\\n' >> \"$LOG\"\n"),
    ] {
        crate::test_executable::write(root.path().join(name), script, 0o755)?;
    }
    let request = Request::new("penso@box", "Mini 'n $HOME", "")?;
    let command = setup_command(
        OsStr::new("sheprd"),
        root.path().join("herdr").as_os_str(),
        &request,
    );
    let words = after_session(&command);
    for (ssh_exit, expected) in [("0", "path\nsave\n"), ("1", "path\n")] {
        let _ = std::fs::remove_file(&log);
        let status = Command::new(words[0])
            .args(&words[1..])
            .env("PATH", root.path())
            .env("LOG", &log)
            .env("SSH_EXIT", ssh_exit)
            .status()?;
        assert_eq!(status.success(), ssh_exit == "0");
        assert_eq!(std::fs::read_to_string(&log)?, expected);
    }
    Ok(())
}

/// A setup that fails, as SSH refused by macOS does, used to leave its host
/// claimed for 15 minutes with nothing on screen, so adding it again said it
/// was "already being added". The relay now holds the claim and releases it
/// the moment the setup ends.
#[test]
fn a_failed_setup_releases_its_host_for_another_try() -> Result<()> {
    use crate::setup_relay::listen;
    use std::io::{Read, Write};
    let root = tempfile::tempdir()?;
    let claim = Claim::acquire(destination("failed-setup.test"), "default")?;
    let mut relay: Relay = listen(
        &root.path().join("gpui"),
        Command::new("/usr/bin/false"),
        Duration::from_secs(30),
    )?;
    relay.keep(claim);
    // The dialog is gone, but the setup still runs: the host stays claimed.
    assert!(matches!(
        Claim::acquire(destination("failed-setup.test"), "default"),
        Err(crate::Error::Devices(Error::Adding))
    ));
    let mut pane = std::os::unix::net::UnixStream::connect(relay.path())?;
    drop(relay);
    // Hello: 24 rows, 80 columns, no TERM.
    pane.write_all(&[1, 0, 0, 0, 4, 0, 24, 0, 80])?;
    // Read to the end of the session, which follows the setup's exit.
    pane.read_to_end(&mut Vec::new())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let retry = loop {
        match Claim::acquire(destination("failed-setup.test"), "default") {
            Err(crate::Error::Devices(Error::Adding)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => break result,
        }
    };
    assert!(retry.is_ok());
    Ok(())
}

/// A relay nobody keeps, as when the workspace could not be created, takes
/// no claim: the dialog keeps it for a retry.
#[test]
fn an_unkept_relay_never_holds_the_host() -> Result<()> {
    let root = tempfile::tempdir()?;
    let relay: Relay = crate::setup_relay::listen(
        &root.path().join("gpui"),
        Command::new("/usr/bin/true"),
        Duration::from_secs(30),
    )?;
    let claim = Claim::acquire(destination("unkept.test"), "default")?;
    drop(relay);
    drop(claim);
    assert!(Claim::acquire(destination("unkept.test"), "default").is_ok());
    Ok(())
}
