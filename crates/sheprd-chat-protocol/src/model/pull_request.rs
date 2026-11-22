//! Pull requests the agent worked on in this chat: found in its tool calls
//! by the chat program, or named by the agent with `sheprd chat pr add`.
//! Only GitHub's own pull request pages are kept, each once, by the address
//! [`PullRequest::url`] rebuilds from the repository and number.

use serde::{Deserialize, Serialize};

/// The most pull requests a transcript keeps; a new one past it is refused.
pub const MAX_PULL_REQUESTS: usize = 64;

const HOST: &str = "https://github.com/";
/// GitHub's own limits on an account and a repository name.
const MAX_OWNER: usize = 39;
const MAX_REPO: usize = 100;

/// One pull request, by its GitHub repository and number.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullRequest {
    /// What the agent did to it, each once, in the order first seen; empty
    /// when it was only named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PullRequestAction>,
    pub number: u64,
    /// `OWNER/REPO`.
    pub repo: String,
    /// `https://github.com/OWNER/REPO/pull/NUMBER`, as [`Self::url_of`]
    /// writes it.
    pub url: String,
}

/// Something the agent did to a pull request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestAction {
    Created,
    /// Commits pushed to its branch.
    Pushed,
    Edited,
    Commented,
    Reviewed,
    /// Marked ready for review.
    Ready,
    Merged,
    Closed,
    Reopened,
    /// An action from a newer program.
    #[serde(other)]
    Other,
}

impl PullRequest {
    /// Pull request `number` of `repo` (`OWNER/REPO`), when both are ones
    /// GitHub could name.
    pub fn new(repo: &str, number: u64, actions: Vec<PullRequestAction>) -> Option<Self> {
        let (owner, name) = repo.split_once('/')?;
        if !valid_owner(owner) || !valid_repo(name) || number == 0 {
            return None;
        }
        Some(Self {
            actions,
            number,
            repo: repo.to_owned(),
            url: Self::url_of(repo, number),
        })
    }

    /// The pull request a GitHub address names, such as
    /// `https://github.com/OWNER/REPO/pull/7`, also with a page, query, or
    /// anchor after the number. Anything else is refused: another host, a
    /// scheme other than `https`, or a path that is not a pull request.
    pub fn parse(url: &str) -> Option<Self> {
        let (pull, rest) = Self::scan(url)?;
        rest.is_empty().then_some(pull)
    }

    /// The pull request an address at the start of `text` names, and the
    /// text after the number, which must not continue the number or a word.
    pub fn scan(text: &str) -> Option<(Self, &str)> {
        let path = text.strip_prefix(HOST)?;
        let mut parts = path.splitn(4, '/');
        let (owner, repo, kind, tail) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        if kind != "pull" {
            return None;
        }
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        let (number, after) = tail.split_at(digits);
        let number = number.parse().ok()?;
        let suffix = after
            .find(|ch: char| ch.is_whitespace() || matches!(ch, ')' | ']' | '>' | '"' | '\'' | '`'))
            .unwrap_or(after.len());
        let (page, rest) = after.split_at(suffix);
        // A page, query, or anchor, or punctuation ending a sentence.
        if !(page.is_empty() || page.starts_with(['/', '#', '?', '.', ',', ';', ':'])) {
            return None;
        }
        let pull = Self::new(&format!("{owner}/{repo}"), number, Vec::new())?;
        Some((pull, rest))
    }

    /// The address of pull request `number` of `repo`.
    pub fn url_of(repo: &str, number: u64) -> String {
        format!("{HOST}{repo}/pull/{number}")
    }

    /// Whether every field is one [`Self::new`] would make, with each action
    /// once: what the reducer takes.
    pub fn fits(&self) -> bool {
        Self::new(&self.repo, self.number, Vec::new()).is_some_and(|valid| valid.url == self.url)
            && self
                .actions
                .iter()
                .enumerate()
                .all(|(index, action)| !self.actions[..index].contains(action))
    }

    /// Whether `other` is the same pull request: GitHub reads owners and
    /// repositories in any case.
    pub fn is(&self, other: &Self) -> bool {
        self.number == other.number && self.repo.eq_ignore_ascii_case(&other.repo)
    }

    /// Adds `actions` it does not have yet; whether any was new.
    pub fn record(&mut self, actions: &[PullRequestAction]) -> bool {
        let before = self.actions.len();
        for action in actions {
            if !self.actions.contains(action) {
                self.actions.push(*action);
            }
        }
        self.actions.len() != before
    }
}

/// A GitHub account: letters, digits, and single inner hyphens.
fn valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= MAX_OWNER
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-')
}

/// A GitHub repository: letters, digits, `.`, `-`, and `_`, but not a
/// name that reads as a directory.
fn valid_repo(repo: &str) -> bool {
    !repo.is_empty()
        && repo.len() <= MAX_REPO
        && repo != "."
        && repo != ".."
        && repo
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}
