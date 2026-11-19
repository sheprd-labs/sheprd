use super::*;

fn log() -> Log {
    let mut tagged = commit('b', '2', "two\twith a tab");
    tagged.change_id = Some("I0123".into());
    let mut merge = commit('c', '0', "");
    merge.patch_id = None;
    Log {
        stack: vec![commit('a', '1', "one"), tagged, merge],
        markers: vec![
            Marker {
                time: 12,
                old: sha('d'),
                outcome: replaced('a', Match::Patch),
                by: Some("Claude Code, Codex".into()),
            },
            Marker {
                time: 13,
                old: sha('e'),
                outcome: Outcome::Dropped,
                by: None,
            },
        ],
    }
}

#[test]
fn the_history_file_reads_back_what_was_written() {
    let mut written = log();
    // Stored subjects are already one clean line.
    written.stack[1].subject = log::clean(&written.stack[1].subject);
    assert_eq!(Log::parse(&written.render()).unwrap(), written);
    assert_eq!(Log::parse("").unwrap(), Log::default());
}

#[test]
fn a_history_file_from_elsewhere_is_refused() {
    for text in [
        "something else\n",
        "herdr-evolution 1\ncommit nothex - - one\n",
        "herdr-evolution 1\nmarker x aaaa - dropped \n",
        &format!("herdr-evolution 1\nmarker 1 {} - patch \n", sha('a')),
        &format!(
            "herdr-evolution 1\nmarker 1 {} {} rebase \n",
            sha('a'),
            sha('b')
        ),
        "herdr-evolution 1\nunknown line\n",
    ] {
        assert!(
            matches!(Log::parse(text), Err(crate::Error::Evolution(Error::Log))),
            "{text:?}"
        );
    }
}

#[test]
fn markers_are_bounded_and_keep_their_commits_once() {
    let mut log = Log::default();
    let markers = (0..log::MARKER_LIMIT + 3).map(|index| Marker {
        time: i64::try_from(index).unwrap(),
        old: sha(if index % 2 == 0 { 'a' } else { 'b' }),
        outcome: Outcome::Dropped,
        by: None,
    });
    log.record(markers);
    assert_eq!(log.markers.len(), log::MARKER_LIMIT);
    assert_eq!(log.markers[0].time, 3, "the oldest go first");
    assert_eq!(log.kept(), [sha('b'), sha('a')]);
}

#[test]
fn a_look_records_only_what_changed() {
    let stack = vec![commit('a', '1', "one")];
    let read = |head: Option<char>, before: Vec<Commit>, stack: Option<Vec<Commit>>| script::Read {
        head: head.map(sha),
        log: Log {
            stack: before,
            markers: Vec::new(),
        },
        stack,
    };
    let codex = Credit::Agents("Codex".into());

    // The first look records the stack, with nothing replaced yet.
    let Plan::Write { old: None, log } = plan(read(None, vec![], Some(stack.clone())), &codex, 5)
    else {
        panic!("first look writes");
    };
    assert_eq!(
        (log.stack.as_slice(), log.markers.len()),
        (stack.as_slice(), 0)
    );

    // The same stack again writes nothing; the default branch neither.
    assert_eq!(
        plan(
            read(Some('f'), stack.clone(), Some(stack.clone())),
            &codex,
            5
        ),
        Plan::Nothing
    );
    assert_eq!(
        plan(read(Some('f'), stack.clone(), None), &codex, 5),
        Plan::Nothing
    );

    // An amend is credited to the turn that just ended.
    let amended = vec![commit('b', '2', "one")];
    let Plan::Write { old, log } = plan(read(Some('f'), stack.clone(), Some(amended)), &codex, 5)
    else {
        panic!("an amend writes");
    };
    assert_eq!(old, Some(sha('f')));
    assert_eq!(
        log.markers,
        [Marker {
            time: 5,
            old: sha('a'),
            outcome: replaced('b', Match::Subject),
            by: Some("Codex".into()),
        }]
    );

    // Merged: no commits of its own left, so its history goes.
    assert_eq!(
        plan(read(Some('f'), stack, Some(vec![])), &codex, 5),
        Plan::Delete { old: sha('f') }
    );
    assert_eq!(
        plan(read(None, vec![], Some(vec![])), &codex, 5),
        Plan::Nothing
    );
}

#[test]
fn a_repeated_section_is_refused() {
    let output = "\u{1e}head\n\n\u{1e}log\n\u{1e}skip\n\u{1e}log\n";
    assert!(matches!(
        script::parse_read(output),
        Err(crate::Error::Evolution(Error::Output))
    ));
}

#[test]
fn new_change_ids_are_gerrit_shaped_and_matchable() {
    let id = new_change_id([0xab; 20]);
    assert_eq!(id, format!("I{}", "ab".repeat(20)));
    assert_eq!(log::change_id_of(&id), Some(id));
}
