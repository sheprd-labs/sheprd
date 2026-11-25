//! Distinguish an unavailable checkout from a confirmed non-Git directory.

use sheprd_client::ConnectTarget;
use sheprd_remote::teleport::Host;
use sheprd_vcs::git::Identity;
use std::{sync::atomic::AtomicBool, time::Instant};

pub(super) enum Lookup {
    Ready(Option<Identity>),
    Unavailable,
}

pub(super) fn read(target: &ConnectTarget, cwd: &str, deadline: Instant) -> Lookup {
    if !target.is_remote() {
        return match sheprd_vcs::git::identity(cwd, deadline, &|| false) {
            Ok(identity) => Lookup::Ready(identity),
            Err(_) => Lookup::Unavailable,
        };
    }
    let Ok(host) = Host::new(target) else {
        return Lookup::Unavailable;
    };
    let cancelled = AtomicBool::new(false);
    match sheprd_remote::work::identity_before(&host, cwd, deadline, &cancelled) {
        Ok(identity) => Lookup::Ready(Some(identity)),
        Err(_) if outside_repository(&host, cwd, deadline) => Lookup::Ready(None),
        Err(_) => Lookup::Unavailable,
    }
}

// Read on the owning POSIX host. No diagnostics are parsed, and an inaccessible
// path or a Git marker (including a dangling symlink) is not a negative result.
const OUTSIDE: &str = r#"
[ -z "${GIT_DIR:-}" ] || exit 1
case "$1" in /*) ;; *) exit 1 ;; esac
cd "$1" || exit 1
path=$(pwd -P) || exit 1
while :; do
    [ -r "$path" ] && [ -x "$path" ] || exit 1
    [ ! -e "$path/.git" ] && [ ! -L "$path/.git" ] || exit 1
    [ "$path" = / ] && break
    path=${path%/*}
    [ -n "$path" ] || path=/
done
printf 'outside\n'
"#;

fn outside_repository(host: &Host, cwd: &str, deadline: Instant) -> bool {
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return false;
    };
    host.exec(
        "sh",
        &["-c", OUTSIDE, "sheprd-checkout", cwd],
        remaining,
        &AtomicBool::new(false),
    )
    .is_ok_and(|output| output == b"outside\n")
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::core_terminals::checkouts::lookup::outside_repository;
    use sheprd_client::ConnectTarget;
    use sheprd_remote::teleport::Host;
    use std::time::{Duration, Instant};

    #[test]
    fn remote_discovery_distinguishes_plain_missing_and_broken_checkouts() {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new(&ConnectTarget::Local).unwrap();
        let probe = |path: &std::path::Path| {
            outside_repository(
                &host,
                path.to_str().unwrap(),
                Instant::now() + Duration::from_secs(5),
            )
        };
        assert!(probe(root.path()));
        assert!(!probe(&root.path().join("missing")));
        std::fs::write(root.path().join(".git"), "broken git file").unwrap();
        let nested = root.path().join("src/nested");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(!probe(&nested));
    }
}
