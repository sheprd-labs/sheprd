//! Git configuration read from a repository on a saved host, over bounded,
//! noninteractive SSH.

use super::{quote, run_remote};
use crate::{Result, catalog::validate_target};
use std::time::Duration;

/// The `remote.origin.url` of a repository on a saved host, read without a
/// prompt. `git_dir` is the absolute Git directory the daemon reported for the
/// workspace. `None` when the repository has no origin remote. Blocks for at
/// most `timeout`: call it from a background thread.
pub fn remote_origin_url(
    target: &str,
    git_dir: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>> {
    remote_config_value(target, git_dir, "remote.origin.url", timeout, cancelled)
}

/// Read one Git configuration value on a saved host using bounded,
/// noninteractive SSH. `dir` is an absolute Git directory or a directory in a
/// working tree, since a daemon reports a workspace's Git directory only for
/// some checkouts. Call only from a background thread.
pub fn remote_config_value(
    target: &str,
    dir: &str,
    key: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>> {
    validate_target(target)?;
    if !dir.starts_with('/') || dir.chars().any(char::is_control) {
        return Err(crate::Error::InvalidGitDir);
    }
    let (status, output) = run_remote(target, &config_command(dir, key), timeout, cancelled)?;
    match status.code() {
        Some(0) => {}
        // `git config --get` exits 1 when the key is absent.
        Some(1) => return Ok(None),
        _ => return Err(crate::Error::RemoteCommand(status)),
    }
    let value = String::from_utf8(output).map_err(|_| crate::Error::RemoteOutput)?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(crate::Error::RemoteOutput);
    }
    Ok(Some(value.to_owned()))
}

pub(super) fn config_command(dir: &str, key: &str) -> String {
    // `-C` rather than `--git-dir`: Git finds the repository from either a
    // Git directory or a working directory, and exits 1 outside one.
    let script = format!(
        "exec git -c core.fsmonitor=false -C {} config --get -- {}",
        quote(dir),
        quote(key)
    );
    format!("/bin/sh -c {}", quote(&script))
}
