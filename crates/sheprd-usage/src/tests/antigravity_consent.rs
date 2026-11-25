use super::*;
use crate::probe::Shell;

const REPORT: &str = r#"{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[{"name":"Gemini Models","buckets":[{"id":"gemini-5h","window":"5h","remaining_fraction":0.5}]}]}}}"#;

/// A local `sh` standing in for a host with a fake `agy` on the PATH that
/// records each run, plus optional `uname` and `security` stubs.
struct AgyHost {
    root: tempfile::TempDir,
}

impl AgyHost {
    fn new(os: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        crate::test_executable::write(
            bin.join("uname"),
            format!("#!/bin/sh\necho '{os}'\n"),
            0o755,
        )
        .unwrap();
        let host = Self { root };
        host.set_agy("1.5.15", format!("printf '%s' '{REPORT}'"));
        host
    }

    fn set_agy(&self, version: &str, usage_body: String) {
        let bin = self.root.path().join("home/.local/bin");
        crate::test_executable::write(
            bin.join("agy"),
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\ncase \"$1\" in --version) echo '{version}';; *) {usage_body};; esac\n",
                log = self.root.path().join("log").display()
            ),
            0o755,
        )
        .unwrap();
    }

    fn set_security(&self, found: bool) {
        let bin = self.root.path().join("home/.local/bin");
        let exit_code = if found { 0 } else { 44 };
        crate::test_executable::write(
            bin.join("security"),
            format!(
                "#!/bin/sh\nprintf 'security %s\\n' \"$*\" >> '{log}'\nexit {exit_code}\n",
                log = self.root.path().join("security.log").display()
            ),
            0o755,
        )
        .unwrap();
    }

    fn dir(&self, path: &str) {
        std::fs::create_dir_all(self.root.path().join("home").join(path)).unwrap();
    }

    fn file(&self, path: &str, contents: &str) {
        let full = self.root.path().join("home").join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, contents).unwrap();
    }

    fn fetch(&self, consent: Consent) -> Option<crate::Result<Report>> {
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-s")
            .env_clear()
            .env("HOME", self.root.path().join("home"))
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut exec = Exec::Remote(Shell::start(command).unwrap());
        let mut jar = CookieJar::default();
        let antigravity = provider("antigravity");
        let mut probe = Probe::new(&mut exec, antigravity, None, &mut jar, consent);
        antigravity.service().fetch(&mut probe)
    }

    fn runs(&self) -> String {
        std::fs::read_to_string(self.root.path().join("log")).unwrap_or_default()
    }

    fn security_runs(&self) -> String {
        std::fs::read_to_string(self.root.path().join("security.log")).unwrap_or_default()
    }
}

/// Only the Antigravity desktop app folder exists (`~/.gemini/antigravity`):
/// a merely detected desktop install must never run `agy` (#320).
#[test]
fn detected_desktop_only_antigravity_never_runs_agy() {
    let host = AgyHost::new("Darwin");
    host.dir(".gemini/antigravity");
    host.set_security(false);
    assert!(host.fetch(Consent::Quiet).is_none());
    assert_eq!(
        host.runs(),
        "",
        "agy was run for a desktop-only install without sign-in"
    );
}

/// Even when `~/.gemini/antigravity-cli` exists, a signed-out install on macOS
/// (no keychain entry, even with stale cache files left after `/logout`) must
/// never run `agy`.
#[test]
fn signed_out_macos_antigravity_never_runs_agy() {
    let host = AgyHost::new("Darwin");
    host.file(
        ".gemini/antigravity-cli/cache/acp-auth-state.json",
        r#"{"methodId":"oauth-personal"}"#,
    );
    host.file(
        ".gemini/antigravity-cli/cache/onboarding.json",
        r#"{"onboardingComplete":true}"#,
    );
    host.set_security(false);
    assert!(host.fetch(Consent::Quiet).is_none());
    assert_eq!(host.runs(), "", "agy was run after keychain sign-out");
    assert!(
        host.security_runs()
            .contains("security find-generic-password -s gemini -a antigravity"),
        "expected metadata-only keychain check, got: {}",
        host.security_runs()
    );
    assert!(
        !host.security_runs().contains(" -w"),
        "keychain existence check must never pass -w: {}",
        host.security_runs()
    );
}

/// A signed-in `agy` CLI on macOS (data dir + keychain entry) is detected
/// automatically without `show_providers`, like Codex and Claude.
#[test]
fn signed_in_macos_antigravity_is_detected_automatically() {
    let host = AgyHost::new("Darwin");
    host.dir(".gemini/antigravity-cli");
    host.set_security(true);
    let report = host.fetch(Consent::Quiet).unwrap().unwrap();
    assert_eq!(report.windows.len(), 1);
    assert!(host.runs().contains("-p /usage --output-format json"));
    assert!(
        !host.security_runs().contains(" -w"),
        "keychain existence check must never pass -w: {}",
        host.security_runs()
    );
}

/// An API-key ACP sign-in has no account quota and must not run `agy`.
#[test]
fn api_key_antigravity_is_skipped() {
    let host = AgyHost::new("Darwin");
    host.file(
        ".gemini/antigravity-cli/cache/acp-auth-state.json",
        r#"{"methodId":"gemini-api-key"}"#,
    );
    host.set_security(true);
    assert!(host.fetch(Consent::Quiet).is_none());
    assert_eq!(host.runs(), "");
}

/// On Linux/WSL, completed onboarding or ACP OAuth sign-in enables automatic
/// detection while a bare `antigravity-cli` folder stays quiet.
#[test]
fn signed_in_linux_antigravity_is_detected_automatically() {
    let host = AgyHost::new("Linux");
    host.dir(".gemini/antigravity-cli");
    assert!(host.fetch(Consent::Quiet).is_none());
    assert_eq!(host.runs(), "");

    host.file(
        ".gemini/antigravity-cli/cache/acp-auth-state.json",
        r#"{"methodId":"oauth-personal"}"#,
    );
    let report = host.fetch(Consent::Quiet).unwrap().unwrap();
    assert_eq!(report.windows.len(), 1);
    assert!(host.runs().contains("-p /usage --output-format json"));
}

/// An older or non-CLI `agy` (such as an IDE launcher) is quietly ignored on
/// auto-detection, while an explicitly listed provider still reports the error.
#[test]
fn incompatible_agy_is_quiet_unless_requested() {
    let host = AgyHost::new("Darwin");
    host.dir(".gemini/antigravity-cli");
    host.set_security(true);
    host.set_agy("1.96.2\nabcdef\narm64", format!("printf '%s' '{REPORT}'"));
    assert!(host.fetch(Consent::Quiet).is_none());
    assert!(matches!(
        host.fetch(Consent::Ask { browsers: false }),
        Some(Err(Error::Command(_)))
    ));
}

/// The usage wrapper shadows `open` and `xdg-open` and sets `BROWSER=false`
/// so a revoked sign-in cannot pop open a browser window.
#[test]
fn report_script_blocks_browser_launchers() {
    let host = AgyHost::new("Darwin");
    host.dir(".gemini/antigravity-cli");
    host.set_security(true);
    let marker = host.root.path().join("browser_check");
    host.set_agy(
        "1.5.15",
        format!(
            "open https://example.com; o=$?; xdg-open https://example.com; x=$?; printf '%s:%s:%s' \"$o\" \"$x\" \"$BROWSER\" > '{}'; printf '%s' '{REPORT}'",
            marker.display()
        ),
    );
    let report = host.fetch(Consent::Quiet).unwrap().unwrap();
    assert_eq!(report.windows.len(), 1);
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "1:1:false");
}

#[test]
fn listed_antigravity_reads_the_agy_report() {
    let host = AgyHost::new("Darwin");
    host.dir(".gemini/antigravity");
    let report = host
        .fetch(Consent::Ask { browsers: false })
        .unwrap()
        .unwrap();
    assert_eq!(report.windows.len(), 1);
    assert!(host.runs().contains("-p /usage --output-format json"));
}
