//! Landing a pull request through GitHub's asynchronous merge API: merging a
//! stacked one, which also merges every open layer below it, and adding one
//! (with those layers, for a stack) to its base branch's merge queue. It is
//! the only merge API that handles stacks and queues.
//!
//! The request is sent once and names the head commit the user saw. GitHub
//! may answer at once or accept it to run in the background; then its result
//! is read again, a bounded number of times, and a merge still running when
//! that ends, or whose result cannot be read once accepted, is reported as
//! such rather than retried or called a failure.

use super::{Outcome, Target};
use crate::pull_request::{self, MergeMethod, clean};
use serde_json::{Value, json};
use std::time::Duration;

/// How often a background merge's result is read, and how many times.
pub(super) const POLL_EVERY: Duration = Duration::from_secs(2);
pub(super) const POLLS: usize = 30;
/// The longest message kept from GitHub's answer.
const MESSAGE_LIMIT: usize = 240;

/// Where a pull request lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// Merged now with `MergeMethod`, with the open layers below it.
    Stack(MergeMethod),
    /// Added to the base branch's merge queue.
    Queue,
}

/// One step the landing asks of its transport, so tests can see each one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    /// `PUT` of `body` to `path`, sent once.
    Put { path: String, body: Value },
    /// A read of `path`.
    Get { path: String },
    /// Waiting before the next read.
    Pause(Duration),
}

/// Lands `target` through `send`, which answers a [`Step::Put`] or
/// [`Step::Get`] with GitHub's JSON and a [`Step::Pause`] with anything.
pub(super) fn land(
    target: &Target,
    landing: Landing,
    mut send: impl FnMut(Step) -> crate::Result<Value>,
) -> crate::Result<Outcome> {
    let path = format!(
        "/repos/{}/{}/pulls/{}/merge-async",
        target.owner, target.repo, target.number
    );
    let body = match landing {
        Landing::Stack(method) => json!({
            "sha": target.head,
            "merge_action": "direct_merge",
            "merge_method": method.graphql().to_ascii_lowercase(),
        }),
        Landing::Queue => json!({"sha": target.head, "merge_action": "merge_queue"}),
    };
    let mut answer = send(Step::Put {
        path: path.clone(),
        body,
    })?;
    for _ in 0..POLLS {
        if answer["status"] != "pending" {
            break;
        }
        let Some(uuid) = answer["details"]["uuid"]
            .as_str()
            .filter(|uuid| request_id(uuid))
        else {
            break;
        };
        let read = format!("{path}/{uuid}");
        // GitHub accepted the request, so a read that fails says nothing of
        // the merge, which may still land: report it accepted, never failed.
        let read = send(Step::Pause(POLL_EVERY)).and_then(|_| send(Step::Get { path: read }));
        match read {
            Ok(next) => answer = next,
            Err(error) => {
                tracing::warn!(
                    category = "github_pr_land",
                    kind = crate::log::kind(&error),
                    "an accepted landing's result could not be read"
                );
                return Ok(Outcome {
                    message: format!(
                        "GitHub accepted landing #{}, but its result could not be read; check it on GitHub",
                        target.number
                    ),
                    url: Some(target.url.clone()),
                });
            }
        }
    }
    outcome(&answer, target, landing)
}

/// What GitHub's answer comes to. A merge it is still running is a success
/// that says so: reporting failure would invite a second merge.
pub(super) fn outcome(answer: &Value, target: &Target, landing: Landing) -> crate::Result<Outcome> {
    let number = target.number;
    let message = match answer["status"].as_str() {
        Some("merged") => match landing {
            Landing::Stack(_) => format!("Merged #{number} and the open pull requests below it"),
            Landing::Queue => format!("Pull request #{number} is already merged"),
        },
        Some("enqueued") => format!("Added #{number} to the merge queue"),
        Some("pending") => format!("GitHub is still landing #{number}; check it on GitHub"),
        Some("failed") => {
            let reason: String = clean(answer["details"]["message"].as_str().unwrap_or_default())
                .trim()
                .chars()
                .take(MESSAGE_LIMIT)
                .collect();
            return Err(crate::Error::Rejected(if reason.is_empty() {
                format!("GitHub could not land #{number}")
            } else {
                reason
            }));
        }
        _ => return Err(pull_request::Error::Identity.into()),
    };
    Ok(Outcome {
        message,
        url: Some(target.url.clone()),
    })
}

/// A request ID as GitHub issues them: a UUID, so it is safe in a path.
fn request_id(uuid: &str) -> bool {
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests;
