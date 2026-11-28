//! The real host scripts against a stand-in `herdr` and `uname` in a
//! private home: what each request carries, and how each answer, refusal,
//! and failure is read.

use crate::plugin_actions::{
    Error, Scope, Target,
    bridge::{self, Invocation},
};
use crate::teleport::Host;
use serde_json::{Value, json};
use sheprd_client::{
    ConnectTarget,
    scrollback::{SelectionReadParams, TextPoint},
};
use std::{os::unix::fs::PermissionsExt, path::Path, sync::atomic::AtomicBool};

/// Answers `remote-api-bridge` requests from files the test writes: one per
/// method, under `answers/`. Records the arguments and each request line.
/// `HERDR_EXIT` in `exit` makes it fail instead.
const HERDR: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/args"
if [ -f "$HOME/exit" ]; then exit "$(cat "$HOME/exit")"; fi
IFS= read -r request || [ -n "$request" ]
printf '%s\n' "$request" >> "$HOME/requests"
case "$request" in
  *'"session.snapshot"'*) cat "$HOME/answers/session.snapshot" ;;
  *'"pane.selection.read"'*) cat "$HOME/answers/pane.selection.read" ;;
  *'"plugin.action.invoke"'*) cat "$HOME/answers/plugin.action.invoke" ;;
  *'"plugin.action.list"'*) cat "$HOME/answers/plugin.action.list" ;;
  *'"plugin.list"'*) cat "$HOME/answers/plugin.list" ;;
esac
"#;

/// A home whose `.local/bin`, first on every host script's `PATH`, holds the
/// stand-in `herdr` and an `uname` that prints `uname`'s file.
pub(crate) struct FakeHerdr {
    pub(crate) home: tempfile::TempDir,
}

impl FakeHerdr {
    pub(crate) fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(home.path().join("answers")).unwrap();
        for (name, body) in [
            ("herdr", HERDR),
            ("uname", "#!/bin/sh\ncat \"$HOME/uname\"\n"),
        ] {
            let path = bin.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let fake = Self { home };
        fake.set_uname("Darwin\n");
        fake
    }

    pub(crate) fn path(&self) -> &Path {
        self.home.path()
    }

    pub(crate) fn set_uname(&self, uname: &str) {
        std::fs::write(self.path().join("uname"), uname).unwrap();
    }

    /// What the stand-in prints for `method`.
    pub(crate) fn answer(&self, method: &str, output: &str) {
        std::fs::write(self.path().join("answers").join(method), output).unwrap();
    }

    pub(crate) fn result(&self, method: &str, result: Value) {
        self.answer(
            method,
            &format!("{}\n", json!({"id": "sheprd", "result": result})),
        );
    }

    fn exit_with(&self, status: Option<u8>) {
        let path = self.path().join("exit");
        match status {
            Some(status) => std::fs::write(path, status.to_string()).unwrap(),
            None => drop(std::fs::remove_file(path)),
        }
    }

    /// The requests the stand-in read, in order.
    pub(crate) fn requests(&self) -> Vec<Value> {
        std::fs::read_to_string(self.path().join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn methods(&self) -> Vec<String> {
        self.requests()
            .iter()
            .map(|request| request["method"].as_str().unwrap().to_owned())
            .collect()
    }

    fn host(&self, target: &ConnectTarget) -> Host {
        let mut host = bridge::host_for(target).unwrap();
        host.env = vec![
            ("HOME".into(), self.path().to_string_lossy().into_owned()),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ];
        host
    }
}

/// Herdr's snapshot: its focus on `w1:p1`; `w1:p2` in another tab.
pub(crate) fn host_snapshot(label: &str) -> Value {
    json!({"snapshot": {
        "workspaces": [{"workspace_id": "w1", "label": label, "active_tab_id": "w1:t1"}],
        "tabs": [
            {"tab_id": "w1:t1", "workspace_id": "w1", "label": "server"},
            {"tab_id": "w1:t2", "workspace_id": "w1", "label": "tests"},
        ],
        "panes": [
            {"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1", "cwd": "/src/api", "agent": "claude"},
            {"pane_id": "w1:p2", "workspace_id": "w1", "tab_id": "w1:t2", "cwd": "/src/api/tests", "agent": "codex"},
        ],
        "layouts": [
            {"tab_id": "w1:t1", "focused_pane_id": "w1:p1"},
            {"tab_id": "w1:t2", "focused_pane_id": "w1:p2"},
        ],
    }})
}

fn list_answers(fake: &FakeHerdr) {
    fake.result(
        "plugin.action.list",
        json!({"actions": [
            {"plugin_id": "hunk", "action_id": "review", "title": "Review", "contexts": ["pane"]},
            {"plugin_id": "hunk", "action_id": "mac", "title": "Mac", "contexts": ["pane"], "platforms": ["macos"]},
            {"plugin_id": "hunk", "action_id": "linux", "title": "Linux", "contexts": ["pane"], "platforms": ["linux"]},
        ]}),
    );
    fake.result(
        "plugin.list",
        json!({"plugins": [{"plugin_id": "hunk", "name": "Hunk", "enabled": true}]}),
    );
}

fn listed(fake: &FakeHerdr) -> crate::plugin_actions::Result<Vec<String>> {
    let host = fake.host(&ConnectTarget::Local);
    let catalog = bridge::fetch(&host, &AtomicBool::new(false))?;
    Ok(catalog
        .rows(Scope::Pane { selection: false })
        .into_iter()
        .map(|row| row.action_id)
        .collect())
}

/// One script reads the host's platform, then both lists, in that order.
#[test]
fn a_list_reads_the_platform_then_actions_then_plugins() {
    let fake = FakeHerdr::new();
    list_answers(&fake);
    assert_eq!(listed(&fake).unwrap(), ["mac", "review"]);
    assert_eq!(fake.methods(), ["plugin.action.list", "plugin.list"]);
    fake.set_uname("Linux\n");
    assert_eq!(listed(&fake).unwrap(), ["linux", "review"]);
    // Not valid UTF-8: the platform is unknown, and Herdr decides.
    std::fs::write(fake.path().join("uname"), b"\xff\xfe\n").unwrap();
    assert_eq!(listed(&fake).unwrap(), ["linux", "mac", "review"]);
}

/// Each way a list can fail is its own error, never a panic or an empty list
/// mistaken for an answer.
#[test]
fn a_failed_list_says_why() {
    let fake = FakeHerdr::new();
    list_answers(&fake);
    let refusal = r#"{"id":"sheprd-plugin-actions","error":{"code":"unknown_method","message":"unknown method: plugin.action.list"}}"#;
    fake.answer("plugin.action.list", refusal);
    assert!(matches!(
        listed(&fake),
        Err(Error::Refused { code, message })
            if code == "unknown_method" && message == "unknown method: plugin.action.list"
    ));
    list_answers(&fake);
    fake.answer("plugin.list", "");
    assert!(matches!(listed(&fake), Err(Error::Unanswered)));
    fake.answer("plugin.list", "{\"id\":\"sheprd-plugins\"}\n");
    assert!(matches!(listed(&fake), Err(Error::Unanswered)));
    fake.answer("plugin.list", "{\"result\": [oops\n");
    assert!(matches!(listed(&fake), Err(Error::Decode(_))));
    list_answers(&fake);
    fake.exit_with(Some(127));
    assert!(matches!(listed(&fake), Err(Error::MissingCli(_))));
    fake.exit_with(Some(3));
    assert!(matches!(listed(&fake), Err(Error::Script(_))));
}

/// A host that prints more than the bound fails the read rather than
/// holding all of it.
#[test]
fn a_list_larger_than_the_bound_fails() {
    let fake = FakeHerdr::new();
    list_answers(&fake);
    let huge = fake.path().join("answers/plugin.action.list");
    let file = std::fs::File::create(&huge).unwrap();
    file.set_len(17 * 1024 * 1024).unwrap();
    assert!(matches!(
        listed(&fake),
        Err(Error::Script(sheprd_client::Error::ScriptOutputLimit))
    ));
}

fn invocation(target: Target, selection: Option<SelectionReadParams>) -> Invocation {
    Invocation {
        plugin_id: "hunk".into(),
        action_id: "review".into(),
        target,
        selection,
    }
}

fn selection(pane: &str) -> SelectionReadParams {
    SelectionReadParams {
        pane_id: pane.into(),
        anchor: TextPoint { row: 4, col: 0 },
        cursor: TextPoint { row: 5, col: 11 },
        content_revision: Some(12),
    }
}

fn invoke(fake: &FakeHerdr, invocation: &Invocation) -> crate::plugin_actions::Result<()> {
    let host = fake.host(&ConnectTarget::Session {
        name: "work".into(),
        development: false,
    });
    bridge::invoke(&host, invocation, &AtomicBool::new(false))
}

/// An action reads Herdr's snapshot, then the selection, then runs with the
/// target's own context. Labels and selected text reach Herdr as data on
/// stdin, whatever shell syntax they hold, and every request goes to the
/// endpoint's session.
#[test]
fn an_action_runs_with_its_targets_context_as_data() {
    let fake = FakeHerdr::new();
    let hostile = "it's $(touch pwned) `touch pwned2`\nnext";
    fake.result("session.snapshot", host_snapshot(hostile));
    fake.result(
        "pane.selection.read",
        json!({"pane_id": "w1:p2", "text": "$(touch pwned3) 'quoted'"}),
    );
    fake.result("plugin.action.invoke", json!({"type": "ok"}));
    invoke(
        &fake,
        &invocation(Target::Pane("w1:p2".into()), Some(selection("w1:p2"))),
    )
    .unwrap();
    let requests = fake.requests();
    assert_eq!(
        fake.methods(),
        [
            "session.snapshot",
            "pane.selection.read",
            "plugin.action.invoke"
        ]
    );
    assert_eq!(
        requests[1]["params"],
        serde_json::to_value(selection("w1:p2")).unwrap()
    );
    assert_eq!(
        requests[2]["params"],
        json!({
            "plugin_id": "hunk",
            "action_id": "review",
            "context": {
                "workspace_id": "w1",
                "workspace_label": hostile,
                "workspace_cwd": "/src/api/tests",
                "tab_id": "w1:t2",
                "tab_label": "tests",
                "focused_pane_id": "w1:p2",
                "focused_pane_cwd": "/src/api/tests",
                "focused_pane_agent": "codex",
                "selected_text": "$(touch pwned3) 'quoted'",
                "invocation_source": "context_menu",
            },
        })
    );
    for planted in ["pwned", "pwned2", "pwned3"] {
        assert!(!fake.path().join(planted).exists(), "{planted} ran");
    }
    let args = std::fs::read_to_string(fake.path().join("args")).unwrap();
    assert!(
        args.lines()
            .all(|line| line == "--session work remote-api-bridge"),
        "{args}"
    );
}

/// Nothing runs when Herdr no longer has the target, or answers a selection
/// read for another pane; Herdr's own refusal keeps its words.
#[test]
fn an_action_on_a_gone_target_is_never_sent() {
    let fake = FakeHerdr::new();
    fake.result("session.snapshot", host_snapshot("api"));
    fake.result(
        "pane.selection.read",
        json!({"pane_id": "w1:p1", "text": "focused pane's text"}),
    );
    for invocation in [
        invocation(Target::Pane("w1:p9".into()), None),
        invocation(Target::Tab("w1:t9".into()), None),
        invocation(Target::Pane("w1:p2".into()), Some(selection("w1:p2"))),
    ] {
        assert!(
            matches!(invoke(&fake, &invocation), Err(Error::TargetGone)),
            "{invocation:?}"
        );
    }
    assert!(!fake.methods().iter().any(|m| m == "plugin.action.invoke"));

    // Trailing output after the answer is ignored.
    fake.answer(
        "plugin.action.invoke",
        "{\"id\":\"sheprd\",\"error\":{\"code\":\"plugin_disabled\",\"message\":\"plugin hunk is disabled\"}}\nwarning: bridge closed\n",
    );
    assert!(matches!(
        invoke(&fake, &invocation(Target::Pane("w1:p2".into()), None)),
        Err(Error::Refused { code, message })
            if code == "plugin_disabled" && message == "plugin hunk is disabled"
    ));
}
