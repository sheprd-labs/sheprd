//! Why a plugin action list could not be read, or an action did not run.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The endpoint has no host a script can reach: a custom socket, or a
    /// client without a POSIX shell.
    #[error(
        "Plugin actions need a local session or SSH host; custom socket endpoints are unsupported."
    )]
    UnsupportedHost,
    /// `herdr --session NAME` always addresses the release installation, so
    /// a development session's daemon cannot be reached by name.
    #[error("Plugin actions are not available for development (--dev) sessions.")]
    DevelopmentSession,
    #[error("Could not run the plugin action: the herdr CLI was not found on this host.")]
    MissingCli(#[source] sheprd_client::Error),
    #[error("Could not run the plugin action: {0}")]
    Script(#[source] sheprd_client::Error),
    /// Herdr answered with its own error envelope.
    #[error("Herdr refused the plugin action: {message}")]
    Refused { code: String, message: String },
    #[error("Could not run the plugin action: herdr returned unexpected output.")]
    Decode(#[source] serde_json::Error),
    /// The API bridge closed without an answer.
    #[error("Could not run the plugin action: herdr did not answer.")]
    Unanswered,
    /// The pane, tab, or workspace the menu was opened on is gone.
    #[error("The plugin action's pane, tab, or workspace no longer exists.")]
    TargetGone,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
