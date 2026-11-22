#![allow(clippy::expect_used)]

use super::*;

/// Claude Code's question, as `pane.read` gave it.
const QUESTION: &str = "\
⏺ Ran 1 shell command
──────────────────────────────────────────────
 ☐ Capture

│ Should a tap answer at once, or ask to confirm first?

❯ 1. Answer immediately
     Like tapping a reply button in Messages.
  2. Confirm first
     Show the choice, then a Send button.
  3. Type something.
──────────────────────────────────────────────
  4. Chat about this

Enter to select · ↑/↓ to navigate · Esc to cancel
";

/// Claude Code's permission prompt, framed.
const PERMISSION: &str = "\
╭──────────────────────────────────────────╮
│ Bash command                             │
│                                          │
│   rm -rf build                           │
│   Remove the build folder                │
│                                          │
│ Do you want to proceed?                  │
│   1. Yes                                 │
│ ❯ 2. Yes, and don't ask again for rm     │
│   3. No, and tell Claude what to do (esc)│
╰──────────────────────────────────────────╯
";

/// Codex's approval.
const CODEX: &str = "\
  Would you like to run the following command?

  $ cargo test

› 1. Yes, proceed (y)
  2. Yes, and don't ask again for this command (a)
  3. No, and tell Codex what to do differently (esc)
";

#[test]
fn reads_a_question_its_choices_and_their_details() {
    let prompt = read(QUESTION).expect("a menu");
    assert_eq!(
        prompt.question,
        [
            "Capture",
            "Should a tap answer at once, or ask to confirm first?"
        ]
    );
    assert_eq!(prompt.selected, 0);
    let labels: Vec<_> = prompt.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Answer immediately",
            "Confirm first",
            "Type something.",
            "Chat about this"
        ]
    );
    assert_eq!(
        prompt.choices[0].detail.as_deref(),
        Some("Like tapping a reply button in Messages.")
    );
    assert_eq!(prompt.choices[2].detail, None);
    assert!(prompt.choices[2].text);
    assert!(!prompt.choices[0].text);
}

#[test]
fn reads_a_framed_permission_prompt_with_its_command() {
    let prompt = read(PERMISSION).expect("a menu");
    assert_eq!(
        prompt.question,
        [
            "Bash command",
            "rm -rf build",
            "Remove the build folder",
            "Do you want to proceed?"
        ]
    );
    assert_eq!(prompt.selected, 1);
    assert_eq!(prompt.choices.len(), 3);
    assert_eq!(
        prompt.choices[2].label,
        "No, and tell Claude what to do (esc)"
    );
}

#[test]
fn reads_codex_approvals() {
    let prompt = read(CODEX).expect("a menu");
    assert_eq!(prompt.selected, 0);
    assert_eq!(prompt.choices[0].label, "Yes, proceed (y)");
    assert_eq!(
        prompt.question,
        [
            "Would you like to run the following command?",
            "$ cargo test"
        ]
    );
}

#[test]
fn a_numbered_list_in_an_answer_is_not_a_menu() {
    let answer = "Steps:\n1. Build it\n2. Test it\n3. Ship it\n";
    assert_eq!(read(answer), None);
    // Two markers cannot say which choice is highlighted.
    assert_eq!(read("❯ 1. One\n❯ 2. Two\n"), None);
    assert_eq!(read(""), None);
}

#[test]
fn the_last_menu_on_screen_wins() {
    let screen = format!("❯ 1. Old\n  2. Older\n{CODEX}");
    assert_eq!(
        read(&screen).expect("a menu").choices[0].label,
        "Yes, proceed (y)"
    );
}

#[test]
fn keys_move_the_highlight_then_take_it() {
    let prompt = read(PERMISSION).expect("a menu");
    assert_eq!(keys_to(&prompt, 1), ["enter"]);
    assert_eq!(keys_to(&prompt, 2), ["down", "enter"]);
    assert_eq!(keys_to(&prompt, 0), ["up", "enter"]);
}
