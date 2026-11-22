/// Why a terminal agent's conversation cannot be shown as a chat. The phone
/// keeps showing the terminal instead.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Herdr's answer about the pane did not have the expected shape")]
    Answer(#[source] serde_json::Error),
    #[error("the agent's own process could not be told apart in its pane")]
    Process,
    #[error(
        "Herdr does not know which conversation this agent is in. Install Herdr's integration for it (herdr integration install claude, or codex) and start the agent again"
    )]
    Session,
    #[error("the observer could not start")]
    Start(#[source] std::io::Error),
    #[error("the agent's conversation could not be read on its computer; open the terminal")]
    Unreadable,
    #[error("only Claude Code or Codex in a Herdr 1 pane on macOS or Linux shows as a chat")]
    Unsupported,
    // keep-sorted end
}
