//! Telling which commit replaced which, from two looks at a branch's stack
//! and nothing else: no hook ran when it was rewritten. A commit is matched
//! only on evidence (its `Change-Id`, its patch, or its subject in the same
//! place); anything left over is dropped rather than guessed.

use super::log::{Commit, Marker, Match, Outcome};
use std::collections::HashSet;

/// What became of each commit of `before` that is not in `after`, in
/// `before`'s order. Each commit of `after` succeeds at most one.
pub fn evolve(before: &[Commit], after: &[Commit]) -> Vec<(String, Outcome)> {
    let kept: HashSet<&str> = after.iter().map(|commit| commit.sha.as_str()).collect();
    let existed: HashSet<&str> = before.iter().map(|commit| commit.sha.as_str()).collect();
    let gone: Vec<(usize, &Commit)> = before
        .iter()
        .enumerate()
        .filter(|(_, commit)| !kept.contains(commit.sha.as_str()))
        .collect();
    let mut fresh: Vec<Option<(usize, &Commit)>> = after
        .iter()
        .enumerate()
        .filter(|(_, commit)| !existed.contains(commit.sha.as_str()))
        .map(Some)
        .collect();
    let mut outcomes: Vec<Option<Outcome>> = vec![None; gone.len()];

    type Rule = fn((usize, &Commit), (usize, &Commit)) -> bool;
    let rules: [(Match, Rule); 3] = [
        (Match::ChangeId, |(_, old), (_, new)| {
            old.change_id.is_some() && old.change_id == new.change_id
        }),
        (Match::Patch, |(_, old), (_, new)| {
            old.patch_id.is_some() && old.patch_id == new.patch_id
        }),
        (Match::Subject, |(at, old), (place, new)| {
            at == place && !old.subject.is_empty() && old.subject == new.subject
        }),
    ];
    for (how, rule) in rules {
        for (index, old) in gone.iter().enumerate() {
            if outcomes[index].is_some() {
                continue;
            }
            let found = fresh
                .iter_mut()
                .find(|candidate| (**candidate).is_some_and(|new| rule(*old, new)))
                .and_then(Option::take);
            if let Some((_, new)) = found {
                outcomes[index] = Some(Outcome::Replaced {
                    new: new.sha.clone(),
                    how,
                });
            }
        }
    }
    gone.into_iter()
        .zip(outcomes)
        .map(|((_, old), outcome)| (old.sha.clone(), outcome.unwrap_or(Outcome::Dropped)))
        .collect()
}

/// The earlier versions of `sha`, newest first: each marker that replaced
/// one version by the next, followed back from `sha`. A commit that came
/// back (an amend undone) is followed once.
pub fn versions<'a>(markers: &'a [Marker], sha: &str) -> Vec<&'a Marker> {
    let mut chain = Vec::new();
    let mut seen = HashSet::from([sha]);
    let mut current = sha;
    while let Some(marker) = markers.iter().rev().find(|marker| {
        matches!(&marker.outcome, Outcome::Replaced { new, .. } if new == current)
            && !seen.contains(marker.old.as_str())
    }) {
        seen.insert(marker.old.as_str());
        current = &marker.old;
        chain.push(marker);
    }
    chain
}
