use super::*;
use crate::pull_request::Tone;

#[test]
fn a_queued_pull_request_reads_queued_rather_than_ready() {
    let mut wire = response();
    wire[0]["mergeStateStatus"] = "CLEAN".into();
    wire[0]["isInMergeQueue"] = true.into();
    let mut pr = parse(&wire.to_string(), "example", "project", "feature")
        .unwrap()
        .unwrap();
    assert!(pr.is_in_merge_queue);
    assert_eq!(pr.tone(), Tone::Queued);
    // The lifecycle still wins: a merged pull request has left the queue.
    pr.state = State::Merged;
    assert_eq!(pr.tone(), Tone::Merged);
}

#[test]
fn a_response_without_the_queue_field_is_not_queued() {
    let mut wire = response();
    wire[0]["mergeStateStatus"] = "CLEAN".into();
    let pr = parse(&wire.to_string(), "example", "project", "feature")
        .unwrap()
        .unwrap();
    assert!(!pr.is_in_merge_queue);
    assert_eq!(pr.tone(), Tone::Ready);
}

#[test]
fn the_queued_tone_travels_as_its_name() {
    assert_eq!(serde_json::to_value(Tone::Queued).unwrap(), "queued");
    let tone: Tone = serde_json::from_value("queued".into()).unwrap();
    assert_eq!(tone, Tone::Queued);
}
