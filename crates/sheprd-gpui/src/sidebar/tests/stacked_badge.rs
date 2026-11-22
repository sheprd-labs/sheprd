//! A worktree row's pull request badge says which layer of a GitHub stack it is.
use crate::sidebar::row::PrBadge;

#[test]
fn a_stacked_pull_request_shows_its_layer_beside_its_number() {
    let theme = crate::config::Theme::default();
    let plain = |text: &str| text.replace(['\u{2068}', '\u{2069}'], "");
    let stacked = PrBadge::new(&crate::pull_request::stacked_fixture().unwrap(), &theme);
    assert_eq!(stacked.number, "#1364");
    assert_eq!(plain(&stacked.label), "#1364 3/4");
    let alone = PrBadge::new(&crate::pull_request::fixture().unwrap(), &theme);
    assert_eq!(alone.label, alone.number);
    let unopened = PrBadge::unopened(&theme);
    assert_eq!(unopened.label, unopened.number);
}

#[test]
fn the_badge_is_measured_without_its_invisible_isolation_marks() {
    let theme = crate::config::Theme::default();
    let stacked = PrBadge::new(&crate::pull_request::stacked_fixture().unwrap(), &theme);
    assert!(
        stacked.label.chars().count() > "#1364 3/4".len(),
        "marks stay in the text"
    );
    assert_eq!(
        crate::sidebar::row::visible_chars(&stacked.label),
        "#1364 3/4".len()
    );
}
