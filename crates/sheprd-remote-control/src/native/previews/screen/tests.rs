use super::*;

const RULE: &str = "──────────────────────────────────────────────────────────────";

fn screen(lines: &[&str]) -> String {
    lines.join("\n")
}

#[test]
fn claude_code_skips_its_prompt_footer_and_status() {
    let text = screen(&[
        "⏺ v1.2.0 is out. CI passed on every platform this time.",
        "",
        "  - Moonframe 1.2.0 is published and marked latest.",
        "",
        "✻ Crunched for 7s · done 11:19 PM",
        "",
        "※ recap: You asked me to publish a new release, and Moonframe 1.2.0",
        "  is out. Next, check that the Homebrew casks install 1.2.0.",
        "                                         new task? /clear to save 105.2k tokens · Update available",
        RULE,
        "❯ ",
        RULE,
        "  ⏵⏵ bypass permissions on (shift+tab to cycle)",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some(
            "recap: You asked me to publish a new release, and Moonframe 1.2.0 \
             is out. Next, check that the Homebrew casks install 1.2.0."
        )
    );
}

#[test]
fn claude_code_while_working_shows_the_tool_call() {
    let text = screen(&[
        "⏺ Bash(cargo test -p sheprd-remote-control)",
        "  ⎿  Running tests…",
        "",
        "✶ Thinking… (12s · esc to interrupt)",
        "",
        RULE,
        "❯ ",
        RULE,
        "  ? for shortcuts",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some("Bash(cargo test -p sheprd-remote-control)\nRunning tests…")
    );
}

#[test]
fn opencode_skips_its_box_status_bar_and_stats() {
    let text = screen(&[
        "     The monitor stopped after GitHub returned 403 API rate limit exceeded.",
        "     The relay page itself was live at the last check.",
        "",
        "     Build · GPT-6 Luna · 11.8s · 42.8 tok/s",
        "",
        "  ┃",
        "  ┃  Build · GPT-6 Luna GitHub Copilot · high",
        "  ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
        "  ~/code/herdr-gpui:main                                       192.3K (19%)",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some(
            "The monitor stopped after GitHub returned 403 API rate limit exceeded. \
             The relay page itself was live at the last check."
        )
    );
}

#[test]
fn codex_skips_its_prompt_and_working_line() {
    let text = screen(&[
        "• Updated the parser and its tests.",
        "",
        "• Working (5s • esc to interrupt)",
        "",
        "› Ask Codex to do anything",
        "",
        "  100% context left · ? for shortcuts",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some("Updated the parser and its tests.")
    );
}

#[test]
fn list_items_stay_on_their_own_lines() {
    let text = screen(&["- First item", "  wrapped under it", "- Second item"]);
    assert_eq!(
        preview(&text).as_deref(),
        Some("First item wrapped under it\nSecond item")
    );
}

#[test]
fn colors_and_hidden_text_are_dropped() {
    let text = "\x1b[1mBold\x1b[0m and \x1b[8msecret\x1b[0m plain\x1b]0;title\x07";
    assert_eq!(preview(text).as_deref(), Some("Bold and  plain"));
}

#[test]
fn an_empty_or_interface_only_screen_has_none() {
    assert_eq!(preview(""), None);
    assert_eq!(preview(&screen(&["", RULE, "❯ ", RULE, ""])), None);
}

#[test]
fn a_long_paragraph_keeps_its_end_and_is_bounded() {
    let line = "word ".repeat(30);
    let lines: Vec<String> = (0..40).map(|n| format!("{n} {line}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let preview = preview(&screen(&lines)).unwrap();
    assert!(preview.starts_with(&format!("{} ", 40 - MAX_PARAGRAPH)));
    assert!(preview.chars().count() <= MAX_CHARS);
    assert!(preview.ends_with('…'));
}

#[test]
fn claude_codes_recap_hint_is_dropped() {
    let text = screen(&[
        "※ recap: The release is out. (disable recaps in /config)",
        RULE,
        "❯ ",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some("recap: The release is out.")
    );
}

#[test]
fn a_codex_approval_shows_its_question_not_its_choices() {
    let text = screen(&[
        "• I'll run the tests before pushing.",
        "",
        "  Would you like to run the following command?",
        "",
        "  $ cargo test",
        "",
        "› 1. Yes, proceed (y)",
        "  2. Yes, and don't ask again for this command (a)",
        "  3. No, and tell Codex what to do differently (esc)",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some(
            "• I'll run the tests before pushing.\n\
             Would you like to run the following command?\n$ cargo test"
        )
    );
}

#[test]
fn a_claude_code_permission_shows_its_question_with_the_choice_moved() {
    let text = screen(&[
        "╭──────────────────────────────────────────╮",
        "│ Bash command                             │",
        "│                                          │",
        "│   rm -rf build                           │",
        "│                                          │",
        "│ Do you want to proceed?                  │",
        "│   1. Yes                                 │",
        "│ ❯ 2. Yes, and don't ask again for rm     │",
        "│   3. No, and tell Claude what to do (esc)│",
        "╰──────────────────────────────────────────╯",
    ]);
    assert_eq!(
        preview(&text).as_deref(),
        Some("Bash command\nrm -rf build\nDo you want to proceed?")
    );
}
