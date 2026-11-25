#![allow(clippy::unwrap_used)]

use super::*;

fn herdr(pane: &str) -> Place {
    Place::Herdr {
        endpoint: "local".to_owned(),
        pane: pane.to_owned(),
    }
}

fn core(key: &str) -> Place {
    Place::Core {
        key: key.to_owned(),
    }
}

fn text<'a>(drafts: &'a Drafts, place: &Place, chat: &str) -> Option<&'a str> {
    drafts.text(place, chat).map(|(text, _)| text)
}

#[test]
fn a_draft_belongs_to_its_chat_and_sending_removes_it() {
    let mut drafts = Drafts::default();
    let seen = drafts.set(&herdr("p1"), "chat-a", "half a thought", None);
    assert!(seen.is_some());
    assert_eq!(
        text(&drafts, &herdr("p1"), "chat-a"),
        Some("half a thought")
    );
    // Another chat later in the same pane does not get it.
    assert_eq!(text(&drafts, &herdr("p1"), "chat-b"), None);
    assert!(drafts.marks.contains(&herdr("p1")));
    // The same text again changes nothing.
    drafts.dirty = false;
    assert_eq!(
        drafts.set(&herdr("p1"), "chat-a", "half a thought", seen),
        seen
    );
    assert!(!drafts.dirty);
    // Sending clears the composer, which removes the draft and its pencil.
    assert_eq!(drafts.set(&herdr("p1"), "chat-a", "  \n", seen), None);
    assert_eq!(text(&drafts, &herdr("p1"), "chat-a"), None);
    assert!(drafts.marks.is_empty());
}

#[test]
fn a_composer_that_never_saw_the_newer_draft_cannot_clear_it() {
    let mut drafts = Drafts::default();
    // Both windows show the first text.
    let first = drafts.set(&herdr("p1"), "chat-a", "draft", None);
    let mut other = first;
    // One window edits it further.
    let newer = drafts.set(&herdr("p1"), "chat-a", "draft, edited", first);
    assert_ne!(newer, first);
    // The other sends its older text: the newer draft stays.
    other = drafts.set(&herdr("p1"), "chat-a", "", other);
    assert_eq!(other, None);
    assert_eq!(text(&drafts, &herdr("p1"), "chat-a"), Some("draft, edited"));
    // The window that wrote it may still send it.
    assert_eq!(drafts.set(&herdr("p1"), "chat-a", "", newer), None);
    assert_eq!(text(&drafts, &herdr("p1"), "chat-a"), None);
}

#[test]
fn another_chat_in_the_pane_replaces_the_old_draft() {
    let mut drafts = Drafts::default();
    drafts.set(&herdr("p1"), "chat-a", "old", None);
    drafts.set(&herdr("p1"), "chat-b", "", None);
    assert!(drafts.drafts.is_empty());
}

#[test]
fn the_marks_are_shared_until_a_pencil_comes_or_goes() {
    let mut drafts = Drafts::default();
    let seen = drafts.set(&herdr("p1"), "chat-a", "h", None);
    let marks = drafts.marks.clone();
    let seen = drafts.set(&herdr("p1"), "chat-a", "he", seen);
    drafts.set(&herdr("p1"), "chat-a", "hey", seen);
    assert!(Arc::ptr_eq(&marks, &drafts.marks));
    drafts.set(&core("core:t1"), "chat-c", "x", None);
    assert!(!Arc::ptr_eq(&marks, &drafts.marks));
    assert_eq!(drafts.marks.len(), 2);
}

#[test]
fn edits_wait_for_a_flush_before_reaching_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let mut drafts = Drafts::at(Some(path.clone()));
    drafts.set(&herdr("p1"), "chat-a", "typed", None);
    assert!(drafts.dirty, "an edit is not written at once");
    drafts.flush();
    assert!(!drafts.dirty);
    drafts.finish();
    assert_eq!(
        text(&Drafts::at(Some(path)), &herdr("p1"), "chat-a"),
        Some("typed")
    );
}

#[test]
fn drafts_are_bounded() {
    let mut drafts = Drafts::default();
    let long = "é".repeat(MAX_TEXT_BYTES);
    drafts.set(&herdr("p1"), "chat-a", &long, None);
    let kept = text(&drafts, &herdr("p1"), "chat-a").unwrap();
    assert!(kept.len() <= MAX_TEXT_BYTES && long.starts_with(kept));
    for index in 0..MAX_DRAFTS * 2 {
        drafts.set(&herdr(&format!("pane-{index}")), "chat", "text", None);
    }
    assert_eq!(drafts.drafts.len(), MAX_DRAFTS);
    // The least recently edited went first.
    assert_eq!(text(&drafts, &herdr("p1"), "chat-a"), None);
    let field = "x".repeat(MAX_FIELD_BYTES + 1);
    assert_eq!(drafts.set(&herdr(&field), "chat", "text", None), None);
    assert_eq!(drafts.set(&herdr("p2"), &field, "text", None), None);
    assert_eq!(drafts.set(&herdr("p2"), "", "text", None), None);
    assert_eq!(text(&drafts, &herdr("p2"), ""), None);
}

#[test]
fn drafts_and_their_versions_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let mut drafts = Drafts::at(Some(path.clone()));
    drafts.set(&herdr("p1"), "chat-a", "line one\nline two", None);
    let last = drafts.set(&core("core:t1"), "chat-c", "core", None);
    drafts.finish();
    let mut restored = Drafts::at(Some(path));
    assert_eq!(
        text(&restored, &herdr("p1"), "chat-a"),
        Some("line one\nline two")
    );
    assert_eq!(
        restored.text(&core("core:t1"), "chat-c"),
        Some(("core", last.unwrap()))
    );
    assert_eq!(restored.marks.len(), 2);
    // New edits keep counting from the saved ones.
    let next = restored.set(&herdr("p2"), "chat-b", "new", None);
    assert!(next > last);
}

#[test]
fn damaged_or_invalid_files_keep_nothing_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    std::fs::write(&path, b"{not json").unwrap();
    assert!(Drafts::at(Some(path.clone())).drafts.is_empty());
    // A blank draft is dropped, and of two in one place the later wins; a
    // file from before versions reads as version zero.
    std::fs::write(
        &path,
        br#"{"drafts":[
            {"place":{"kind":"herdr","endpoint":"local","pane":"p1"},"chat":"a","text":"old"},
            {"place":{"kind":"core","key":"core:t1"},"chat":"c","text":"  "},
            {"place":{"kind":"herdr","endpoint":"local","pane":"p1"},"chat":"b","text":"new"}
        ]}"#,
    )
    .unwrap();
    let drafts = Drafts::at(Some(path));
    assert_eq!(drafts.drafts.len(), 1);
    assert_eq!(drafts.text(&herdr("p1"), "b"), Some(("new", 0)));
    assert_eq!(drafts.marks.len(), 1);
}
