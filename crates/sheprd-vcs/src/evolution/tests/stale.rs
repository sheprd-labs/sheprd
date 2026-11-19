use super::*;

#[test]
fn a_branch_on_a_replaced_version_is_found_with_its_rebase() {
    let history = History {
        stack: vec![commit('c', '3', "two")],
        markers: vec![
            Marker {
                time: 0,
                old: sha('a'),
                outcome: replaced('b', Match::Subject),
                by: None,
            },
            Marker {
                time: 0,
                old: sha('b'),
                outcome: replaced('c', Match::Patch),
                by: None,
            },
            Marker {
                time: 0,
                old: sha('d'),
                outcome: Outcome::Dropped,
                by: None,
            },
        ],
        pending: 0,
    };
    let branches = vec![
        ("stack/up".to_owned(), vec![sha('9'), sha('b'), sha('a')]),
        ("on drop".to_owned(), vec![sha('d')]),
        ("fresh".to_owned(), vec![sha('8'), sha('c')]),
    ];
    let stale = find_stale(&history, &branches);
    assert_eq!(
        stale,
        [
            Stale {
                branch: "stack/up".into(),
                old: sha('b'),
                new: Some(sha('c')),
            },
            Stale {
                branch: "on drop".into(),
                old: sha('d'),
                new: None,
            },
        ],
        "the nearest old version; a branch on the current one is fine"
    );
    assert_eq!(
        stale[0].command().unwrap(),
        format!(
            "git rebase --onto {} {} stack/up",
            &sha('c')[..12],
            &sha('b')[..12]
        )
    );
    assert_eq!(stale[1].command(), None);
    let odd = Stale {
        branch: "it's".into(),
        ..stale[0].clone()
    };
    assert!(odd.command().unwrap().ends_with(" 'it'\\''s'"));
}
