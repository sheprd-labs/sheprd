//! Claude's compaction: a boundary record, then a summary written as the
//! person's message, which the chat folds into one notice.

use super::*;

const SUMMARY: &str = "This session is being continued from a previous conversation that ran out of context.\n\n<system-reminder>\n<tools-used-before-summary>Bash, Edit</tools-used-before-summary>\n</system-reminder>\nSummary:\n1. Primary Request and Intent";

fn boundary(uuid: &str) -> Value {
    json!({"type":"system","subtype":"compact_boundary","uuid":uuid,"parentUuid":null,
        "logicalParentUuid":"before","content":"Conversation compacted","isMeta":false,
        "level":"info","compactMetadata":{"trigger":"auto","preTokens":337_200}})
}

fn summary(uuid: &str) -> Value {
    json!({"type":"user","uuid":uuid,"isCompactSummary":true,"isVisibleInTranscriptOnly":true,
        "message":{"role":"user","content":SUMMARY}})
}

fn user(uuid: &str, text: &str) -> Value {
    json!({"type":"user","uuid":uuid,"message":{"role":"user","content":text}})
}

#[test]
fn a_compaction_summary_is_a_notice_never_the_persons_prompt() {
    let bytes = lines(&[
        user("u1", "Make Moonframe the default"),
        boundary("b1"),
        summary("s1"),
        user("u2", "Carry on"),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    let prompts: Vec<_> = transcript
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, ["Make Moonframe the default", "Carry on"]);
    assert_eq!(transcript.entries.len(), 3);
    assert_eq!(
        transcript.entries[1].kind,
        EntryKind::Notice {
            answers: Vec::new(),
            details: Some(SUMMARY.into()),
            level: sheprd_chat_protocol::NoticeLevel::Info,
            text: "Context compacted".into(),
        }
    );
}

#[test]
fn each_compaction_keeps_its_own_summary() {
    let bytes = lines(&[
        boundary("b1"),
        summary("s1"),
        user("u1", "Next"),
        boundary("b2"),
        summary("s2"),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    let notices: Vec<_> = transcript
        .entries
        .iter()
        .filter(|entry| {
            matches!(&entry.kind, EntryKind::Notice { details: Some(details), .. } if details == SUMMARY)
        })
        .collect();
    assert_eq!(notices.len(), 2);
    assert_eq!(transcript.entries.len(), 3);
}

#[test]
fn a_summary_without_its_boundary_still_folds() {
    // The window may start after the boundary record.
    let transcript = parse(TranscriptAgent::Claude, &lines(&[summary("s1")]), 0);
    assert!(matches!(
        &transcript.entries[..],
        [Entry { kind: EntryKind::Notice { details: Some(details), text, .. }, .. }]
            if details == SUMMARY && text == "Context compacted"
    ));
}

#[test]
fn a_boundary_without_a_summary_is_a_plain_notice() {
    let transcript = parse(TranscriptAgent::Claude, &lines(&[boundary("b1")]), 0);
    assert!(matches!(
        &transcript.entries[..],
        [Entry { kind: EntryKind::Notice { details: None, text, .. }, .. }]
            if text == "Context compacted"
    ));
}

#[test]
fn a_sidechain_boundary_is_not_shown() {
    let mut record = boundary("b1");
    record["isSidechain"] = json!(true);
    assert!(
        parse(TranscriptAgent::Claude, &lines(&[record]), 0)
            .entries
            .is_empty()
    );
}
