//! A real process on this Mac: its birth time and working directory come
//! from the kernel, not a fake `/proc`.

use super::*;
use std::process::Child;

/// A child that only waits.
fn agent(cwd: &Path) -> Child {
    sheprd_client::new_command("/bin/sleep")
        .arg("60")
        .current_dir(cwd)
        .spawn()
        .unwrap()
}

fn claude_options(pid: u32, started: u64) -> ObserveOptions {
    ObserveOptions {
        agent: TranscriptAgent::Claude,
        session: SESSION.into(),
        pid,
        started,
        pane: Some("w1:p1".into()),
    }
}

fn open(options: &ObserveOptions, home: &Path) -> Result<identity::Identity> {
    let process = process::Process::local(options.pid)?;
    identity::Identity::of(options, process, Some(home))
}

#[test]
fn a_running_claude_is_identified_by_its_birth_and_default_config_directory() {
    let home = tempfile::tempdir().unwrap();
    let project = home.path().join(".claude/projects/-work-project");
    fs::create_dir_all(&project).unwrap();
    let path = project.join(format!("{SESSION}.jsonl"));
    fs::write(
        &path,
        format!("{{\"type\":\"user\",\"sessionId\":\"{SESSION}\"}}\n"),
    )
    .unwrap();
    let mut child = agent(home.path());
    let pid = child.id();
    let started = process::Process::local(pid).unwrap().started().unwrap();
    assert!(started > 0);

    let found = open(&claude_options(pid, started), home.path()).unwrap();
    assert_eq!(found.path, path);
    assert_eq!(
        found.cwd.as_deref().map(std::path::PathBuf::from),
        Some(home.path().canonicalize().unwrap())
    );
    assert!(found.process_matches().unwrap());

    // Another birth time is another process that reused the PID.
    assert!(matches!(
        open(&claude_options(pid, started + 1), home.path()),
        Err(Error::ProcessChanged)
    ));

    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!found.process_matches().unwrap());
}

#[test]
fn a_process_that_is_gone_is_not_found_rather_than_failed() {
    let mut child = agent(Path::new("/"));
    let pid = child.id();
    child.kill().unwrap();
    child.wait().unwrap();
    let Err(Error::Io(error)) = process::Process::local(pid).unwrap().started() else {
        panic!("a reaped child still has a birth time");
    };
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}
