#![allow(clippy::unwrap_used)]

use super::*;

const ID: &str = "0123456789abcdef";

#[test]
fn host_names_round_trip_and_nothing_else_is_one() {
    let host = host_name(ID);
    assert_eq!(host, "r-0123456789abcdef.sheprd.invalid");
    assert_eq!(host_id(&host), Some(ID));
    for host in [
        "r-0123456789ABCDEF.sheprd.invalid",
        "r-0123456789abcde.sheprd.invalid",
        "x-0123456789abcdef.sheprd.invalid",
        "r-0123456789abcdef.sheprd.example",
        "r-0123456789abcdefsheprd.invalid",
        "example.org",
    ] {
        assert_eq!(host_id(host), None, "{host}");
    }
}

#[test]
fn targets_with_users_schemes_and_ports_are_recognised() {
    let host = host_name(ID);
    for target in [
        host.clone(),
        format!("me@{host}"),
        format!("ssh://me@{host}:22"),
        format!("{host}:2222"),
    ] {
        assert!(is_relay_target(&target), "{target}");
    }
    assert!(!is_relay_target("me@example.org"));
}

fn proxy(state_dir: &str) -> RelayProxy {
    RelayProxy {
        program: PathBuf::from("/Applications/Sheprd.app/Contents/MacOS/sheprd-remote-control"),
        state_dir: PathBuf::from(state_dir),
    }
}

// Windows paths have backslashes, which the proxy refuses: adding a nearby
// Sheprd is not offered there.
#[cfg(unix)]
#[test]
fn a_relay_machine_gets_the_proxy_identity_and_its_own_known_hosts() {
    let options =
        proxy_options(&proxy("/Users/me/Library/Application Support/100%h/remote")).unwrap();
    let values: Vec<&str> = options
        .iter()
        .skip(1)
        .step_by(2)
        .map(String::as_str)
        .collect();
    assert!(options.iter().step_by(2).all(|flag| flag == "-o"));
    assert_eq!(
        values[0],
        "ProxyCommand=\"/Applications/Sheprd.app/Contents/MacOS/sheprd-remote-control\" relay-connect --state-dir \"/Users/me/Library/Application Support/100%%h/remote\" %h"
    );
    assert_eq!(
        values[1],
        "IdentityFile=\"/Users/me/Library/Application Support/100%%h/remote/desktop/id_ed25519\""
    );
    assert!(values.contains(&"IdentitiesOnly=yes"));
    // The user's agent never sees the pairing key (`AddKeysToAgent yes`
    // would hand it over, and gpg-agent then asks a passphrase).
    assert!(values.contains(&"IdentityAgent=none"));
    assert!(values.contains(&"AddKeysToAgent=no"));
    assert!(values.contains(&"StrictHostKeyChecking=yes"));
    assert!(values.contains(&"ControlPersist=no"));
    assert!(values.contains(
        // OpenSSH expands `%h` to the machine's host name: a file each.
        &"UserKnownHostsFile=\"/Users/me/Library/Application Support/100%%h/remote/desktop/known_hosts.d/%h\""
    ));
}

#[test]
fn paths_the_shell_would_expand_are_refused() {
    for state_dir in ["/a/$HOME", "/a/`x`", "/a/\"b", "relative", "/a\\b"] {
        assert_eq!(proxy_options(&proxy(state_dir)), None, "{state_dir}");
    }
}

#[test]
fn other_targets_get_no_options() {
    assert!(options("me@example.org").is_empty());
}

/// With a `ControlPath` and `ControlPersist` in the user's config, OpenSSH
/// would send the proxy's stderr to /dev/null; the relay options keep the
/// ban line `relay-connect` prints, so the closed bridge reads as a ban.
#[cfg(unix)]
#[test]
fn a_ban_line_survives_a_persistent_control_path_in_the_user_config() {
    use crate::SshFailure;
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = tempfile::tempdir().unwrap();
    let state = scratch.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    let program = scratch.path().join("relay-connect");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho '{} probe 540' >&2\nexit 1\n",
            sheprd_relay::ban::LINE_PREFIX
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = scratch.path().join("config");
    std::fs::write(
        &config,
        format!(
            "ControlMaster auto\nControlPath {}/c-%C\nControlPersist 3600\n",
            scratch.path().display()
        ),
    )
    .unwrap();
    let options = proxy_options(&RelayProxy {
        program,
        state_dir: state,
    })
    .unwrap();
    let output = crate::environment::command("ssh")
        .args(&options)
        .arg("-F")
        .arg(&config)
        .args(["-o", "BatchMode=yes", "-o", "ControlMaster=no"])
        .args(["--", &format!("me@{}", host_name(ID)), "true"])
        .output()
        .unwrap();
    assert_eq!(
        SshFailure::classify(output.status.code(), &output.stderr),
        SshFailure::RelayBanned(sheprd_relay::ban::Ban {
            reason: Some(sheprd_relay::ban::BanReason::Probe),
            remaining: Some(std::time::Duration::from_secs(540)),
        }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
