//! Stacks GitHub does not keep, inferred from branch bases: open pull
//! requests of one repository where each one's base is the head of the one
//! below, as `gh stack`, Graphite, or a person stacking by hand leaves them.
//! Only the list already read is used, so nothing is asked of GitHub.
//!
//! A chain is followed only while each link is unambiguous at both ends:
//! the lower pull request is the only one with that head, and the upper the
//! only one opened onto it. A branch two pull requests build on ends the
//! chain there, so every layer of a chain infers the same stack.

use crate::{
    orchestrator::{CheckState, Item, PrStatus, Provider, Review},
    pull_request::{LAYER_LIMIT, Layer, MergeState, Outcome, ReviewDecision, Stack, State},
};
use std::collections::HashMap;

/// The stack `current` is a layer of, with its position from 1 at the
/// bottom, when at least one other open pull request joins it.
pub(in crate::orchestrator::view) fn infer(
    items: &[Item],
    statuses: &HashMap<(String, u64), PrStatus>,
    current: &Item,
) -> Option<(Stack, u64)> {
    let open: Vec<&Item> = items
        .iter()
        .filter(|item| own_open(item) && item.key.source == current.key.source)
        .collect();
    if !own_open(current) {
        return None;
    }
    let mut below: Vec<&Item> = Vec::new();
    let mut at = current;
    while below.len() + 1 < LAYER_LIMIT {
        let base = &at.pull_request.as_ref()?.base_ref;
        let Some(next) = only(open.iter().copied().filter(|item| head(item) == Some(base)))
            .filter(|lower| linked(&open, lower, at))
        else {
            break;
        };
        if next.key == current.key || below.iter().any(|seen| seen.key == next.key) {
            break;
        }
        below.push(next);
        at = next;
    }
    let mut above: Vec<&Item> = Vec::new();
    let mut at = current;
    while below.len() + above.len() + 1 < LAYER_LIMIT {
        let tip = head(at)?;
        let Some(next) = only(open.iter().copied().filter(|item| base(item) == Some(tip)))
            .filter(|upper| linked(&open, at, upper))
        else {
            break;
        };
        if next.key == current.key || below.iter().chain(&above).any(|seen| seen.key == next.key) {
            break;
        }
        above.push(next);
        at = next;
    }
    if below.is_empty() && above.is_empty() {
        return None;
    }
    let position = below.len() as u64 + 1;
    let chain: Vec<&Item> = below
        .iter()
        .rev()
        .copied()
        .chain(std::iter::once(current))
        .chain(above)
        .collect();
    let base = base(chain[0])?.to_owned();
    let layers: Vec<Layer> = chain
        .iter()
        .zip(1..)
        .filter_map(|(item, position)| layer(item, position, statuses))
        .collect();
    Some((
        Stack {
            number: 0,
            base,
            size: layers.len() as u64,
            layers,
        },
        position,
    ))
}

/// Whether `upper` is built on `lower` and on nothing else: `lower` is the
/// only open pull request with `upper`'s base as its head, and `upper` the
/// only one opened onto `lower`'s head.
fn linked(open: &[&Item], lower: &Item, upper: &Item) -> bool {
    let (Some(lower_head), Some(upper_base)) = (head(lower), base(upper)) else {
        return false;
    };
    lower_head == upper_base
        && only(
            open.iter()
                .copied()
                .filter(|item| head(item) == Some(upper_base)),
        )
        .is_some_and(|found| found.key == lower.key)
        && only(
            open.iter()
                .copied()
                .filter(|item| base(item) == Some(lower_head)),
        )
        .is_some_and(|found| found.key == upper.key)
}

/// An open GitHub pull request whose head is known to be in its own
/// repository: a fork's branch names, or those of a head GitHub no longer
/// reports (a deleted fork), say nothing about this repository's branches.
fn own_open(item: &Item) -> bool {
    item.key.source.provider == Provider::Github
        && matches!(item.state.as_str(), "open" | "draft")
        && item.pull_request.as_ref().is_some_and(|pr| {
            pr.head_repository
                .as_deref()
                .is_some_and(|head| head.eq_ignore_ascii_case(&item.key.source.repository))
        })
}

fn head(item: &Item) -> Option<&str> {
    item.pull_request.as_ref().map(|pr| pr.head_ref.as_str())
}

fn base(item: &Item) -> Option<&str> {
    item.pull_request.as_ref().map(|pr| pr.base_ref.as_str())
}

/// The one item an iterator yields, if it yields exactly one.
fn only<'a>(mut items: impl Iterator<Item = &'a Item>) -> Option<&'a Item> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

fn layer(item: &Item, position: u64, statuses: &HashMap<(String, u64), PrStatus>) -> Option<Layer> {
    let number = item.pull_request.as_ref()?.number;
    let status = statuses.get(&(item.key.source.repository.clone(), number));
    Some(Layer {
        position,
        number,
        title: item.title.clone(),
        state: State::Open,
        is_draft: item.state == "draft",
        merge: status.map_or(MergeState::Unknown, |status| status.merge),
        review: status
            .and_then(|status| status.review)
            .map_or(ReviewDecision::None, |review: Review| review.into()),
        checks: status.and_then(|status| rollup(status.checks.iter().map(|check| check.state))),
        // Restacking reads GitHub's own stacks only.
        head_oid: String::new(),
    })
}

/// The checks summed up as GitHub's rollup would: any failure, else anything
/// running, else passed; `None` when none ran.
fn rollup(states: impl Iterator<Item = CheckState> + Clone) -> Option<Outcome> {
    let any = |wanted| states.clone().any(|state| state == wanted);
    if any(CheckState::Failed) {
        Some(Outcome::Failed)
    } else if any(CheckState::Pending) {
        Some(Outcome::Pending)
    } else if states.clone().next().is_some() {
        Some(Outcome::Passed)
    } else {
        None
    }
}
