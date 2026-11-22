//! Open pull requests whose bases are each other's heads read as a stack
//! GitHub does not keep, shown without stack merging.
use super::*;
use crate::{
    orchestrator::{
        CheckState, PrStatus, PullRequest, Review,
        review::Check,
        service::PullRequestLookup,
        view::{rows::Tab, stack::infer},
    },
    pull_request::{Outcome, ReviewDecision},
};
use gpui::{Modifiers, TestAppContext};
use std::{collections::HashMap, sync::Arc};

fn pr(number: u64, base: &str, head: &str) -> Item {
    let mut item = item(
        Provider::Github,
        &format!("pr/{number}"),
        &format!("Layer #{number}"),
        "2026-11-17T00:00:00Z",
    );
    item.pull_request = Some(PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: base.into(),
        head_ref: head.into(),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: Some("penso/app".into()),
    });
    item
}

fn chain() -> Vec<Item> {
    vec![
        pr(2, "feat/a", "feat/b"),
        pr(1, "main", "feat/a"),
        pr(3, "feat/b", "feat/c"),
        pr(9, "main", "fix/other"),
    ]
}

fn numbers(stack: &crate::pull_request::Stack) -> Vec<u64> {
    stack.layers.iter().map(|layer| layer.number).collect()
}

#[test]
fn a_chain_of_bases_reads_bottom_to_top_from_any_layer() {
    let items = chain();
    for (index, position) in [(1, 1), (0, 2), (2, 3)] {
        let (stack, at) = infer(&items, &HashMap::new(), &items[index]).unwrap();
        assert_eq!(numbers(&stack), [1, 2, 3]);
        assert_eq!((stack.base.as_str(), stack.size, at), ("main", 3, position));
    }
    // A pull request nothing builds on, onto the trunk, is in no stack.
    assert!(infer(&items, &HashMap::new(), &items[3]).is_none());
}

#[test]
fn a_fork_in_the_chain_ends_it_where_it_is_ambiguous() {
    let mut items = chain();
    items.push(pr(4, "feat/b", "feat/d"));
    let (stack, at) = infer(&items, &HashMap::new(), &items[0]).unwrap();
    assert_eq!((numbers(&stack), at), (vec![1, 2], 2));
}

#[test]
fn forks_closed_pull_requests_and_other_repositories_do_not_join() {
    let mut items = chain();
    items[1].pull_request.as_mut().unwrap().head_repository = Some("someone/app".into());
    let (stack, _) = infer(&items, &HashMap::new(), &items[0]).unwrap();
    assert_eq!(numbers(&stack), [2, 3]);
    let mut items = chain();
    items[2].state = "merged".into();
    items[1].key.source.repository = "penso/other".into();
    assert!(infer(&items, &HashMap::new(), &items[0]).is_none());
}

#[test]
fn layers_take_their_review_checks_and_draft_state_from_the_list() {
    let mut items = chain();
    items[2].state = "draft".into();
    let statuses = HashMap::from([(
        ("penso/app".to_owned(), 1),
        PrStatus {
            review: Some(Review::Required),
            requested: Vec::new(),
            checks: vec![
                Check {
                    name: "test".into(),
                    state: CheckState::Passed,
                },
                Check {
                    name: "lint".into(),
                    state: CheckState::Failed,
                },
            ],
            merge: crate::pull_request::MergeState::Blocked,
        },
    )]);
    let (stack, _) = infer(&items, &statuses, &items[0]).unwrap();
    let bottom = &stack.layers[0];
    assert_eq!(bottom.review, ReviewDecision::ReviewRequired);
    assert_eq!(bottom.checks, Some(Outcome::Failed));
    assert!(stack.layers[2].is_draft);
    assert_eq!(stack.layers[1].checks, None);
}

#[gpui::test]
fn an_inferred_stack_shows_its_chip_with_a_note_and_no_stack_merge(cx: &mut TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let mut looked_up = crate::pull_request::fixture().unwrap();
    looked_up.number = 2;
    looked_up.url = "https://github.com/penso/app/pull/2".into();
    looked_up.merge_methods = vec![crate::pull_request::MergeMethod::Squash];
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let items = chain();
            let key = items[0].key.canonical();
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(items);
            repo.snapshot.pull_request = Some(PullRequestLookup {
                number: 2,
                result: Arc::new(Ok(Some(looked_up))),
            });
            view.show_list(Tab::PullRequests, cx);
            view.open_detail(key, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let chip = cx.debug_bounds("orchestrator-stack-chip").unwrap();
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-stack-inferred").is_some());
    assert!(cx.debug_bounds("orchestrator-stack-merge").is_none());
    assert!(cx.debug_bounds("orchestrator-stack-enqueue").is_none());
}

#[test]
fn a_split_below_gives_no_stack_from_any_of_its_layers() {
    let items = vec![
        pr(1, "main", "feat/a"),
        pr(2, "feat/a", "feat/b"),
        pr(3, "feat/a", "feat/c"),
    ];
    for current in &items {
        assert!(
            infer(&items, &HashMap::new(), current).is_none(),
            "#{} sees the same answer as the others",
            current.pull_request.as_ref().unwrap().number
        );
    }
}

#[test]
fn a_pull_request_whose_head_repository_is_unknown_never_joins() {
    let mut items = chain();
    items[1].pull_request.as_mut().unwrap().head_repository = None;
    let (stack, at) = infer(&items, &HashMap::new(), &items[0]).unwrap();
    assert_eq!((numbers(&stack), at), (vec![2, 3], 1));
    assert!(infer(&items, &HashMap::new(), &items[1]).is_none());
}
