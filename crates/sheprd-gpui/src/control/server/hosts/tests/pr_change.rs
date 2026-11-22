//! What `pr merge` and `pr queue` refuse before opening a dialog, and why.
use super::super::pr::refusal;
use crate::{
    menu::PrChange,
    pull_request::{
        MergeMethod, QueueEntry, QueueState, State, ready_stacked_fixture, stacked_fixture,
    },
};
use sheprd_control_protocol::protocol::hosts::QueueOp;

fn queued() -> Option<QueueEntry> {
    Some(QueueEntry {
        state: QueueState::Queued,
        position: 1,
        eta: None,
    })
}

#[test]
fn a_ready_stack_can_be_merged_and_a_blocked_one_says_which_layer_blocks() {
    let mut ready = ready_stacked_fixture().unwrap();
    ready.merge_methods = vec![MergeMethod::Squash];
    assert_eq!(refusal(&ready, PrChange::Merge), None);
    let mut blocked = stacked_fixture().unwrap();
    blocked.merge_methods = vec![MergeMethod::Squash];
    assert_eq!(
        refusal(&blocked, PrChange::Merge).as_deref(),
        Some("The stack cannot merge yet: #1362 is checks failed")
    );
}

#[test]
fn closed_draft_queued_and_methodless_pull_requests_are_refused() {
    let mut pr = ready_stacked_fixture().unwrap();
    assert!(
        refusal(&pr, PrChange::Merge)
            .unwrap()
            .contains("no merge method")
    );
    pr.merge_methods = vec![MergeMethod::Merge];
    pr.queue = queued();
    assert!(
        refusal(&pr, PrChange::Merge)
            .unwrap()
            .contains("pr queue remove")
    );
    pr.is_draft = true;
    assert!(refusal(&pr, PrChange::Merge).unwrap().contains("draft"));
    pr.state = State::Merged;
    assert!(
        refusal(&pr, PrChange::Queue(QueueOp::Add))
            .unwrap()
            .contains("not open")
    );
}

#[test]
fn queueing_needs_a_queue_and_removing_needs_an_entry() {
    let mut pr = ready_stacked_fixture().unwrap();
    assert_eq!(refusal(&pr, PrChange::Queue(QueueOp::Add)), None);
    assert!(
        refusal(&pr, PrChange::Queue(QueueOp::Remove))
            .unwrap()
            .contains("not in")
    );
    pr.queue = queued();
    assert!(
        refusal(&pr, PrChange::Queue(QueueOp::Add))
            .unwrap()
            .contains("already")
    );
    assert_eq!(refusal(&pr, PrChange::Queue(QueueOp::Remove)), None);
    pr.merge_queue = false;
    pr.queue = None;
    assert!(
        refusal(&pr, PrChange::Queue(QueueOp::Add))
            .unwrap()
            .contains("no merge queue")
    );
}
