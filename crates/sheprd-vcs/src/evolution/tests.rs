#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod credit;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod git;
mod history_log;
mod matching;
mod stale;

/// A full object name made of one repeated hex digit.
fn sha(digit: char) -> String {
    digit.to_string().repeat(40)
}

fn commit(digit: char, patch: char, subject: &str) -> Commit {
    Commit {
        sha: sha(digit),
        patch_id: Some(sha(patch)),
        change_id: None,
        subject: subject.into(),
    }
}

fn replaced(new: char, how: Match) -> Outcome {
    Outcome::Replaced { new: sha(new), how }
}
