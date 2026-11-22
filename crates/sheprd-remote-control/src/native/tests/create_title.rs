//! The name a phone gives its new workspace.

use crate::{native::create::check_title, web::ApiError};

#[test]
fn a_title_is_trimmed_and_a_blank_one_names_nothing() {
    assert_eq!(check_title("  Review  ").ok(), Some(Some("Review")));
    assert_eq!(check_title(" \t ").ok(), Some(None));
    assert_eq!(check_title("").ok(), Some(None));
}

#[test]
fn an_overlong_title_or_one_with_control_characters_is_refused() {
    let long = "x".repeat(sheprd_session::saved::MAX_LABEL_CHARS + 1);
    assert!(matches!(check_title(&long), Err(ApiError::Invalid(_))));
    assert!(matches!(
        check_title("two\nlines"),
        Err(ApiError::Invalid(_))
    ));
    let longest = "é".repeat(sheprd_session::saved::MAX_LABEL_CHARS);
    assert!(check_title(&longest).is_ok(), "counted in characters");
}
