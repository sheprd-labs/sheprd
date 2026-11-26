use crate::push::plain::{plain, verdict_first};

#[test]
fn size_marks_and_code_marks_leave_their_text() {
    assert_eq!(
        plain(":big[Nothing is uncommitted here.] This worktree (`port/mobile`) is clean"),
        "Nothing is uncommitted here. This worktree (port/mobile) is clean"
    );
}

#[test]
fn markdown_reads_as_one_plain_line() {
    assert_eq!(
        plain("# Done\n\n**Merged** [PR #12](https://example.com/12), ~~old~~ _new_."),
        "Done Merged PR #12, old new."
    );
    assert_eq!(plain("- one\n- two\n\n> quoted"), "one two quoted");
    assert_eq!(plain("```sh\njust ci\n```\nran"), "just ci ran");
    assert_eq!(plain("![a chart](/tmp/c.png) here"), "a chart here");
}

#[test]
fn size_marks_nest_and_only_open_where_the_chat_opens_them() {
    assert_eq!(plain(":huge[a :big[b] c] d"), "a b c d");
    assert_eq!(plain(":big[see [x] here]"), "see [x] here");
    assert_eq!(plain("http://x:big[y]"), "http://x:big[y]");
    assert_eq!(plain("a:big[y]"), "a:big[y]");
    assert_eq!(plain(":big[unclosed"), "unclosed");
    assert_eq!(plain(":tiny[x]"), ":tiny[x]");
}

#[test]
fn size_marks_in_code_stay_text_and_never_lead() {
    assert_eq!(plain("Use `:big[FAIL]` here"), "Use :big[FAIL] here");
    assert_eq!(plain("```\n:huge[x]\n```"), ":huge[x]");
    assert_eq!(
        verdict_first("I checked. Use `:big[FAIL]` in the fixture. :big[All tests pass.]"),
        "All tests pass. I checked. Use :big[FAIL] in the fixture."
    );
}

#[test]
fn a_verdict_after_a_list_without_periods_still_leads() {
    let items = (1..=12)
        .map(|n| format!("- updated module number {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let reply = format!("{items}\n\n:big[All tests pass.] Ready to merge.");
    assert!(
        verdict_first(&reply).starts_with("All tests pass. updated module number 1 "),
        "{}",
        verdict_first(&reply)
    );
    // A heading ends its sentence as a period would.
    assert_eq!(
        verdict_first("# Result\n\nDetails first :big[Green] then more"),
        "Details first Green then more Result"
    );
}
