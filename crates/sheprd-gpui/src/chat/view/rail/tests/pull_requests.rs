use super::*;
use crate::pull_request::Tone;

#[core::prelude::v1::test]
fn a_listed_item_settles_only_merged_and_closed() {
    use crate::chat::view::rail::pull_requests::state_tone;
    assert_eq!(state_tone("MERGED"), Some(Tone::Merged));
    assert_eq!(state_tone("closed"), Some(Tone::Attention));
    assert_eq!(state_tone("open"), None);
    assert_eq!(state_tone(""), None);
}

#[core::prelude::v1::test]
fn the_pull_requests_section_folds_on_its_own_like_the_changed_files() {
    use crate::chat::rail_folds::RailFolds;
    let folds = RailFolds::default();
    assert_eq!(
        folds.is_open(RailSection::PullRequests, true),
        folds.is_open(RailSection::Changed, true)
    );
    assert!(folds.is_open(RailSection::PullRequests, false));
}
