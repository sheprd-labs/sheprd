//! A socket's views stay within [`MAX_VIEWS`], and a view a screen shows
//! takes a background chat's place rather than being refused.

use super::*;

fn chat(index: usize) -> View {
    View::Chat {
        pane: format!("p{index}"),
    }
}

fn term(index: usize) -> View {
    View::Term {
        pane: format!("t{index}"),
    }
}

/// Views full of `background` chats, then terminals a screen shows.
fn full(background: usize) -> Views {
    let mut views = Views::default();
    for index in 0..background {
        views.start(chat(index), true, std::future::pending());
    }
    for index in background..MAX_VIEWS {
        views.start(term(index), false, std::future::pending());
    }
    views
}

#[tokio::test]
async fn a_view_on_screen_takes_a_background_chats_place() {
    let mut views = full(1);
    assert_eq!(views.admit(&term(99), false), Some(Some(chat(0))));
    assert!(!views.running.contains_key(&chat(0)));
    assert!(views.background.is_empty());
}

#[tokio::test]
async fn a_background_chat_never_takes_a_place() {
    let mut views = full(1);
    assert_eq!(views.admit(&chat(99), true), None);
    assert!(views.running.contains_key(&chat(0)));
}

#[tokio::test]
async fn with_no_background_chat_a_full_socket_refuses() {
    let mut views = full(0);
    assert_eq!(views.admit(&term(99), false), None);
    // Replacing a view that runs always fits.
    assert_eq!(views.admit(&term(0), false), Some(None));
}

#[tokio::test]
async fn a_background_chat_opened_on_screen_is_no_longer_background() {
    let mut views = full(1);
    views.start(chat(0), false, std::future::pending());
    assert!(views.background.is_empty());
    assert_eq!(views.admit(&term(99), false), None);
}
