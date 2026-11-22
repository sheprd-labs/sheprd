#![allow(clippy::expect_used)]

use super::*;

#[test]
fn stat_fields_follow_the_last_parenthesis() {
    let fields = (4..=22)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let stat = format!("123 (agent with ) parens) S {fields}\n");
    assert_eq!(
        parse_stat(&stat),
        Some(ProcessIdentity {
            parent: 4,
            group: 5,
            started: 22,
        })
    );
    assert_eq!(parse_stat("123 (truncated"), None);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn this_process_is_read_and_stable() {
    let pid = std::process::id();
    let first = local_identity(pid).expect("this process");
    assert_eq!(local_identity(pid), Some(first));
    assert!(first.started > 0);
}
