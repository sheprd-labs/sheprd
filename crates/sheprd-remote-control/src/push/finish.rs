//! What a terminal agent's finished push says, so it reads on its own on a
//! locked screen: where (the workspace, unless its name was generated),
//! what was asked (the task its terminal title shows), and what the agent
//! says it did (its last paragraph on screen, read once the run settled).
//! Under [`Privacy::Generic`] only where, and that it is done.

use super::{Chat, Kind, Notification, Privacy, terminals::Seen, triggers};

/// What a finish says when nothing more may or can be said.
const DONE: &str = "Done";

/// The finished push before the agent's screen was read: the task as its
/// body, which [`recapped`] moves up when the agent says what it did.
pub(super) fn finished(agent: &Seen<'_>, privacy: Privacy) -> Notification {
    let task = agent
        .task
        .filter(|_| privacy == Privacy::Excerpt)
        .map(triggers::sentences)
        .filter(|task| !task.is_empty());
    let body = task.unwrap_or_else(|| DONE.to_owned());
    let mut finished = Notification::new(
        &agent.chat,
        Kind::TerminalDone,
        place(&agent.chat).to_owned(),
        body,
    );
    finished.subtitle = String::new();
    finished
}

/// `finished` with what the agent says it did, `recap`, as its body and
/// what it was asked, `task`, above it. Unchanged for any other push or
/// an empty recap; the caller reads no screen under [`Privacy::Generic`].
pub(crate) fn recapped(
    mut finished: Notification,
    task: Option<&str>,
    recap: &str,
) -> Notification {
    let recap = triggers::sentences(recap);
    if finished.kind != Kind::TerminalDone || recap.is_empty() {
        return finished;
    }
    finished.subtitle = task.map(triggers::sentences).unwrap_or_default();
    finished.body = recap;
    finished
}

/// The workspace, or the tab when the workspace's name was generated and
/// says nothing, or the agent when neither has a name.
fn place<'a>(chat: &Chat<'a>) -> &'a str {
    [chat.workspace, chat.title]
        .into_iter()
        .find(|name| !name.trim().is_empty() && !generated(name))
        .unwrap_or(chat.agent)
}

/// Whether `name` is one Sheprd or Herdr made up for a worktree, such as
/// `green-meadow-a5eb` or `worktree-green-forest-0c5b`: two words and four
/// hex digits at its end.
pub(super) fn generated(name: &str) -> bool {
    let name = name.rsplit('/').next().unwrap_or(name);
    let mut parts = name.rsplit('-');
    let suffix = parts.next().unwrap_or_default();
    let words = [parts.next(), parts.next()];
    suffix.len() == 4
        && suffix
            .chars()
            .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
        && words.iter().all(|word| {
            word.is_some_and(|word| {
                !word.is_empty() && word.chars().all(|ch| ch.is_ascii_lowercase())
            })
        })
}
