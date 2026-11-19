use super::*;

#[test]
fn an_amend_keeps_its_subject_and_place() {
    let before = [commit('a', '1', "one"), commit('b', '2', "two")];
    let after = [commit('a', '1', "one"), commit('c', '3', "two")];
    assert_eq!(
        matcher::evolve(&before, &after),
        [(sha('b'), replaced('c', Match::Subject))]
    );
}

#[test]
fn a_reword_or_a_rebase_keeps_the_patch() {
    let before = [commit('a', '1', "one"), commit('b', '2', "two")];
    // Rebased onto a new base, the second reworded, and the two reordered.
    let after = [commit('d', '2', "two, reworded"), commit('c', '1', "one")];
    assert_eq!(
        matcher::evolve(&before, &after),
        [
            (sha('a'), replaced('c', Match::Patch)),
            (sha('b'), replaced('d', Match::Patch)),
        ]
    );
}

#[test]
fn a_change_id_wins_over_a_patch() {
    let mut first = commit('a', '1', "one");
    first.change_id = Some("I1".into());
    let mut second = commit('b', '2', "two");
    second.change_id = Some("I2".into());
    // The new commit has the first's patch but the second's Change-Id.
    let mut new = commit('c', '1', "three");
    new.change_id = Some("I2".into());
    assert_eq!(
        matcher::evolve(&[first, second], &[new]),
        [
            (sha('a'), Outcome::Dropped),
            (sha('b'), replaced('c', Match::ChangeId)),
        ]
    );
}

#[test]
fn a_squash_keeps_one_and_drops_the_other() {
    let before = [commit('a', '1', "one"), commit('b', '2', "two")];
    let after = [commit('c', '3', "one")];
    assert_eq!(
        matcher::evolve(&before, &after),
        [
            (sha('a'), replaced('c', Match::Subject)),
            (sha('b'), Outcome::Dropped),
        ]
    );
}

#[test]
fn a_split_succeeds_once_and_adds_the_rest() {
    let before = [commit('a', '1', "one")];
    let after = [commit('b', '2', "one"), commit('c', '3', "one, the rest")];
    assert_eq!(
        matcher::evolve(&before, &after),
        [(sha('a'), replaced('b', Match::Subject))]
    );
}

#[test]
fn nothing_is_guessed() {
    let before = [commit('a', '1', "one"), commit('b', '2', "two")];
    // Same subject, another place; no patch in common.
    let after = [commit('c', '3', "two"), commit('d', '4', "new")];
    assert_eq!(
        matcher::evolve(&before, &after),
        [(sha('a'), Outcome::Dropped), (sha('b'), Outcome::Dropped)]
    );
    // Merges and empty commits have no patch, which matches nothing.
    let mut empty = commit('e', '0', "one");
    empty.patch_id = None;
    let mut other = commit('f', '0', "other");
    other.patch_id = None;
    assert_eq!(
        matcher::evolve(&[empty], &[commit('9', '9', "x"), other]),
        [(sha('e'), Outcome::Dropped)]
    );
    // Unchanged stacks have nothing to say.
    assert_eq!(matcher::evolve(&before, &before), []);
}

fn marker(old: char, new: char) -> Marker {
    Marker {
        time: 0,
        old: sha(old),
        outcome: replaced(new, Match::Subject),
        by: None,
    }
}

#[test]
fn versions_follow_markers_back_newest_first() {
    let markers = [
        marker('a', 'b'),
        Marker {
            outcome: Outcome::Dropped,
            ..marker('9', '9')
        },
        marker('b', 'c'),
    ];
    let chain: Vec<&str> = versions(&markers, &sha('c'))
        .iter()
        .map(|marker| marker.old.as_str())
        .collect();
    assert_eq!(chain, [sha('b'), sha('a')]);
    assert!(versions(&markers, &sha('a')).is_empty());
}

#[test]
fn an_undone_amend_is_followed_once() {
    // a → b, then b undone back to a, then a amended again to c.
    let markers = [marker('a', 'b'), marker('b', 'a'), marker('a', 'c')];
    let chain: Vec<&str> = versions(&markers, &sha('c'))
        .iter()
        .map(|marker| marker.old.as_str())
        .collect();
    assert_eq!(chain, [sha('a'), sha('b')]);
}

#[test]
fn versions_resolve_by_prefix_and_lead_to_what_they_became() {
    let history = History {
        stack: vec![commit('c', '3', "two")],
        markers: vec![
            marker('a', 'b'),
            marker('b', 'c'),
            Marker {
                outcome: Outcome::Dropped,
                ..marker('d', 'd')
            },
        ],
        pending: 0,
    };
    assert_eq!(history.resolve("aaaa").unwrap(), sha('a'));
    assert_eq!(history.resolve("cc").unwrap(), sha('c'));
    assert!(matches!(
        history.resolve("ffff"),
        Err(crate::Error::Evolution(Error::Unknown))
    ));
    assert!(matches!(
        history.resolve(""),
        Err(crate::Error::Evolution(Error::Ambiguous))
    ));
    assert_eq!(history.current(&sha('a')), Some(sha('c').as_str()));
    assert_eq!(history.current(&sha('c')), Some(sha('c').as_str()));
    assert_eq!(history.current(&sha('d')), None, "dropped");
    assert_eq!(history.current(&sha('e')), None, "never seen");
}
