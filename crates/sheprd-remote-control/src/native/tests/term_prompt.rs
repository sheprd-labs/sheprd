//! `GET /api/term/{pane}/prompt`, as the Swift tests decode it: a
//! question with a described choice, a plain one, and one taking words,
//! and a pane with no question.

use super::fixtures::check;
use crate::web::prompt::PromptAnswer;
use sheprd_client::herdr::prompt;

#[test]
fn term_prompt_fixtures_match() {
    let screen = "\
 ☐ Deploy
│ Which environment?
❯ 1. Staging
     The one the team shares.
  2. Production
  3. Type something.
Enter to select · ↑/↓ to navigate · Esc to cancel
";
    check(
        "term_prompt",
        &PromptAnswer {
            prompt: prompt::read(screen),
        },
    );
    check("term_prompt_none", &PromptAnswer { prompt: None });
}
