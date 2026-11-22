//! Answering a terminal agent's question: only the question the phone was
//! shown, and only one of its choices.

use crate::web::prompt::{Choose, Plan, plan};
use sheprd_client::herdr::prompt::{self, Prompt};

fn approval(command: &str) -> Prompt {
    prompt::read(&format!(
        "Bash command\n  {command}\nDo you want to proceed?\n❯ 1. Yes\n  2. No\n  3. Type something.\n"
    ))
    .expect("a menu")
}

fn choose(shown: &Prompt, number: u32, text: Option<&str>) -> Choose {
    Choose {
        question: shown.question.clone(),
        number,
        label: shown.choices[number as usize - 1].label.clone(),
        text: text.map(str::to_owned),
    }
}

#[test]
fn an_answer_takes_the_choice_it_was_shown() {
    let shown = approval("ls");
    assert_eq!(
        plan(&shown, choose(&shown, 2, None)).ok(),
        Some(Plan::Keys(vec!["down", "enter"]))
    );
}

#[test]
fn a_late_answer_never_approves_another_command() {
    let shown = approval("ls");
    let now = approval("rm -rf build");
    // The same "Yes" under the same number, for a command never shown.
    assert!(plan(&now, choose(&shown, 1, None)).is_err());
}

#[test]
fn words_go_into_the_choice_that_takes_them() {
    let shown = approval("ls");
    assert_eq!(
        plan(&shown, choose(&shown, 3, Some("use git clean"))).ok(),
        Some(Plan::Words {
            moves: vec!["down", "down"],
            text: "use git clean".into(),
        })
    );
    assert!(plan(&shown, choose(&shown, 3, Some("  "))).is_err());
}
