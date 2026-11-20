//! Herdr's API socket on the endpoint's host, through `herdr remote-api-bridge`.
//!
//! `plugin.action.list` and `plugin.action.invoke` are not among the methods
//! Herdr admits on a GUI client's connection, and the `herdr plugin action`
//! CLI cannot carry an invocation context. So each request runs Herdr's own
//! API bridge on the host, locally or over SSH, through the same host scripts
//! pane moves use: one JSON request on its stdin, one answer on its stdout.
//! Blocking: call only on a background worker.

use super::{
    Error, Result,
    catalog::{Catalog, Installed, Listed, Platform},
    context::{HostView, Target},
};
use crate::teleport::Host;
use serde::Deserialize;
use serde_json::{Value, json};
use sheprd_client::{
    ScriptLimits,
    scrollback::{SelectionReadParams, SelectionResult},
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The bridge's own script: the request arrives on stdin.
const BRIDGE: &str = "herdr_cli remote-api-bridge\n";

/// One script for the menu's list: the host's platform, which Herdr's
/// platform check uses, then both answers, in one round trip to the host.
/// The requests are constants, so the script carries no caller data.
const LIST: &str = r#"uname -s
herdr_cli remote-api-bridge <<'SHEPRD_ACTIONS'
{"id":"sheprd-plugin-actions","method":"plugin.action.list","params":{}}
SHEPRD_ACTIONS
herdr_cli remote-api-bridge <<'SHEPRD_PLUGINS'
{"id":"sheprd-plugins","method":"plugin.list","params":{}}
SHEPRD_PLUGINS
"#;

/// A session snapshot names every pane; far smaller than this in practice.
const LIMITS: ScriptLimits = ScriptLimits {
    output: 16 * 1024 * 1024,
    idle: Duration::from_secs(15),
};

/// The API methods these scripts call, by their wire names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiMethod {
    // keep-sorted start
    PaneSelectionRead,
    PluginActionInvoke,
    SessionSnapshot,
    // keep-sorted end
}

impl ApiMethod {
    fn name(self) -> &'static str {
        match self {
            Self::PaneSelectionRead => "pane.selection.read",
            Self::PluginActionInvoke => "plugin.action.invoke",
            Self::SessionSnapshot => "session.snapshot",
        }
    }
}

/// One action to run, captured when its row was chosen.
#[derive(Debug, Clone)]
pub(crate) struct Invocation {
    pub(crate) plugin_id: String,
    pub(crate) action_id: String,
    pub(crate) target: Target,
    /// The pane's highlighted cells, for an action that takes a selection.
    pub(crate) selection: Option<SelectionReadParams>,
}

/// `{"result": T}` or `{"error": {"code", "message"}}`, as the socket answers.
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Refusal>,
}

#[derive(Deserialize)]
struct Refusal {
    #[serde(default)]
    code: String,
    message: String,
}

impl Envelope {
    fn into_result(self) -> Result<Value> {
        if let Some(Refusal { code, message }) = self.error {
            return Err(Error::Refused { code, message });
        }
        self.result.ok_or(Error::Unanswered)
    }
}

#[derive(Deserialize)]
struct ActionList {
    actions: Vec<Listed>,
}

#[derive(Deserialize)]
struct PluginList {
    plugins: Vec<Installed>,
}

#[derive(Deserialize)]
struct Snapshot {
    snapshot: HostView,
}

/// The actions `host` offers, enabled and for its platform.
pub(crate) fn fetch(host: &Host, cancelled: &AtomicBool) -> Result<Catalog> {
    let mut output = Vec::new();
    host.stream(LIST, &[][..], &mut output, LIMITS, cancelled)
        .map_err(failure)?;
    let (platform, answers) = output
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|at| output.split_at(at))
        .ok_or(Error::Unanswered)?;
    let platform = std::str::from_utf8(platform)
        .ok()
        .and_then(Platform::of_uname);
    let mut answers = serde_json::Deserializer::from_slice(answers).into_iter::<Envelope>();
    let mut next = || -> Result<Value> {
        answers
            .next()
            .ok_or(Error::Unanswered)?
            .map_err(Error::Decode)?
            .into_result()
    };
    let listed: ActionList = serde_json::from_value(next()?).map_err(Error::Decode)?;
    let installed: PluginList = serde_json::from_value(next()?).map_err(Error::Decode)?;
    Ok(Catalog::new(platform, listed.actions, &installed.plugins))
}

/// Runs `invocation` on `host` with the context Herdr would build for its
/// target, read from Herdr's own snapshot just before.
pub(crate) fn invoke(host: &Host, invocation: &Invocation, cancelled: &AtomicBool) -> Result<()> {
    let view: Snapshot = serde_json::from_value(call(
        host,
        ApiMethod::SessionSnapshot,
        json!({}),
        cancelled,
    )?)
    .map_err(Error::Decode)?;
    let selected_text = match &invocation.selection {
        Some(selection) => {
            let params = serde_json::to_value(selection).map_err(Error::Decode)?;
            let answer = call(host, ApiMethod::PaneSelectionRead, params, cancelled)?;
            let read: SelectionResult = serde_json::from_value(answer).map_err(Error::Decode)?;
            if read.pane_id != selection.pane_id {
                return Err(Error::TargetGone);
            }
            Some(read.text)
        }
        None => None,
    };
    let context = view.snapshot.context(&invocation.target, selected_text)?;
    call(
        host,
        ApiMethod::PluginActionInvoke,
        json!({
            "plugin_id": invocation.plugin_id,
            "action_id": invocation.action_id,
            "context": context,
        }),
        cancelled,
    )
    .map(drop)
}

/// One request on its own bridge, answered with its `result`.
fn call(host: &Host, method: ApiMethod, params: Value, cancelled: &AtomicBool) -> Result<Value> {
    let mut request =
        json!({"id": "sheprd", "method": method.name(), "params": params}).to_string();
    request.push('\n');
    let mut output = Vec::new();
    host.stream(BRIDGE, request.as_bytes(), &mut output, LIMITS, cancelled)
        .map_err(failure)?;
    serde_json::Deserializer::from_slice(&output)
        .into_iter::<Envelope>()
        .next()
        .ok_or(Error::Unanswered)?
        .map_err(Error::Decode)?
        .into_result()
}

/// A shell that could not find `herdr` says so; any other failure keeps the
/// script's error, whose stderr holds the bridge's reason.
fn failure(error: sheprd_client::Error) -> Error {
    match &error {
        sheprd_client::Error::ScriptExit { status, .. } if status.code() == Some(127) => {
            Error::MissingCli(error)
        }
        _ => Error::Script(error),
    }
}

/// The host whose Herdr serves `target`'s session, if a script can reach it:
/// host scripts need a POSIX client, and the `herdr` CLI cannot name a
/// development session or a custom socket.
pub(crate) fn host_for(target: &sheprd_client::ConnectTarget) -> Result<Host> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(Error::UnsupportedHost);
    }
    crate::teleport::host_for(target).map_err(|error| match error {
        crate::teleport::Error::DevelopmentSession => Error::DevelopmentSession,
        _ => Error::UnsupportedHost,
    })
}
