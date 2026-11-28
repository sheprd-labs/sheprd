use crate::thread::{file::*, *};
use crate::{Entry, EntryKind, NoticeLevel};

fn abs(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:{path}"))
    } else {
        PathBuf::from(path)
    }
}

fn record() -> Record {
    let mut thread = Thread::new(ThreadId::random(), Agent::Claude);
    thread.opened("0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10");
    thread.ran("opus");
    thread.switch(Agent::Codex, Some(EntryId(7)));
    Record {
        created_ms: 1,
        cwd: abs("/work/repo"),
        thread,
        title: "Fix the build".to_owned(),
        updated_ms: 2,
    }
}

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            guest: None,
            text: text.to_owned(),
            files: 0,
            note: None,
        },
    }
}

fn agent(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Agent {
            text: text.to_owned(),
        },
    }
}

fn ids(contents: &Contents) -> Vec<u64> {
    contents.entries.iter().map(|entry| entry.id.0).collect()
}

#[test]
fn thread_ids_round_trip_as_text_and_refuse_other_text() {
    let id = ThreadId::random();
    assert_eq!(id.to_string().parse::<ThreadId>().unwrap(), id);
    assert!(matches!("nope".parse::<ThreadId>(), Err(Error::Id)));
}

#[test]
fn a_record_round_trips_privately() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().join("threads");
    let written = record();
    written.write(&dir).unwrap();
    let path = record_path(&dir, &written.thread.id);
    assert_eq!(Record::read(&path).unwrap(), written);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&path), 0o600);
    }
}

#[test]
fn records_that_break_their_bounds_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = record();
    bad.thread.segments.clear();
    assert!(matches!(bad.write(dir.path()), Err(Error::Segments)));
    let mut bad = record();
    bad.cwd = PathBuf::from("relative");
    assert!(matches!(bad.write(dir.path()), Err(Error::Path)));
    let mut bad = record();
    bad.title = "x".repeat(MAX_TITLE + 1);
    assert!(matches!(bad.write(dir.path()), Err(Error::Title)));
    let mut bad = record();
    bad.thread.segments[0].session = Some("a b".to_owned());
    assert!(matches!(bad.write(dir.path()), Err(Error::Session)));
    let mut bad = record();
    bad.thread.segments[0].model = Some("m".repeat(MAX_MODEL + 1));
    assert!(matches!(bad.write(dir.path()), Err(Error::Model)));
}

#[test]
fn a_record_of_another_version_does_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let written = record();
    written.write(dir.path()).unwrap();
    let path = record_path(dir.path(), &written.thread.id);
    let mut stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    stored["version"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert!(matches!(Record::read(&path), Err(Error::Version(2))));
}

#[test]
fn segments_past_the_cap_fold_the_oldest_into_the_next() {
    let mut thread = Thread::new(ThreadId::random(), Agent::Claude);
    for at in 1..=MAX_SEGMENTS as u64 {
        let agent = if at % 2 == 0 {
            Agent::Claude
        } else {
            Agent::Codex
        };
        thread.switch(agent, Some(EntryId(at * 10)));
    }
    assert_eq!(thread.segments.len(), MAX_SEGMENTS);
    // The first segment left; the next one now starts where it did.
    assert_eq!(thread.segments[0].after, None);
    assert_eq!(thread.segments[1].after, Some(EntryId(20)));
    assert_eq!(thread.current().unwrap().after, Some(EntryId(640)));
    thread.validate().unwrap();
}

#[test]
fn a_segment_keeps_only_plain_sessions_and_reports_model_changes() {
    let mut thread = Thread::new(ThreadId::random(), Agent::Claude);
    thread.opened("-rf");
    assert_eq!(thread.current().unwrap().session, None);
    thread.opened("abc-1");
    assert_eq!(thread.current().unwrap().session.as_deref(), Some("abc-1"));
    assert!(thread.ran("opus"));
    assert!(!thread.ran("opus"));
    assert!(thread.ran(&"é".repeat(MAX_MODEL)));
    assert!(thread.current().unwrap().model.as_ref().unwrap().len() <= MAX_MODEL);
}

#[test]
fn a_title_is_the_first_line_with_text_cut_on_a_character() {
    assert_eq!(title("\n  Fix the build  \nthen test"), "Fix the build");
    let long = title(&"é".repeat(MAX_TITLE));
    assert!(long.len() <= MAX_TITLE && long.chars().all(|c| c == 'é'));
    assert_eq!(title(""), "");
}

#[test]
fn threads_live_beside_restore_records() {
    let var = |name: &str| match name {
        "XDG_RUNTIME_DIR" | "HOME" | "TMPDIR" => Some(abs("/tmp/me").into_os_string()),
        _ => None,
    };
    let restore = crate::restore::restore_dir_with(var).unwrap();
    assert_eq!(
        threads_dir_with(var).unwrap(),
        restore.with_file_name("threads")
    );
}

#[test]
fn the_file_keeps_each_entry_at_its_latest() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut writer = Writer::open(dir.path(), &id).unwrap();
    writer
        .append(&[&user(1, "hello"), &agent(2, "hi")])
        .unwrap();
    writer
        .append(&[&agent(2, "hi there"), &agent(3, "more")])
        .unwrap();
    let contents = Contents::read(writer.path()).unwrap();
    assert_eq!(ids(&contents), [1, 2, 3]);
    assert_eq!(contents.entries[1], agent(2, "hi there"));
    assert_eq!(contents.last_id(), Some(EntryId(3)));
    // A writer opened later appends after what is there.
    let mut again = Writer::open(dir.path(), &id).unwrap();
    again.append(&[&agent(4, "after")]).unwrap();
    assert_eq!(ids(&Contents::read(writer.path()).unwrap()), [1, 2, 3, 4]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(writer.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn a_missing_file_holds_nothing_and_a_torn_line_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let path = transcript_path(dir.path(), &id);
    assert_eq!(Contents::read(&path).unwrap(), Contents::default());
    let mut writer = Writer::open(dir.path(), &id).unwrap();
    writer.append(&[&user(1, "hello")]).unwrap();
    // A program killed mid-write leaves half a line, and a newer one may
    // write lines this version does not know.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend(b"{\"line\":\"future\",\"x\":1}\n{\"line\":\"entry\",\"entry\":{\"id\":2,");
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(ids(&Contents::read(&path).unwrap()), [1]);
    // A writer opened on the torn file starts its first line on its own.
    let mut after = Writer::open(dir.path(), &id).unwrap();
    after.append(&[&agent(3, "after the tear")]).unwrap();
    assert_eq!(ids(&Contents::read(&path).unwrap()), [1, 3]);
}

#[test]
fn a_file_that_does_not_read_is_never_trimmed_over() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let path = transcript_path(dir.path(), &id);
    // Past the bound, so the next append trims, which must read it first.
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_FILE + 1)
        .unwrap();
    let mut writer = Writer::open(dir.path(), &id).unwrap();
    assert!(matches!(
        writer.append(&[&user(1, "hello")]),
        Err(Error::TranscriptSize)
    ));
    assert_eq!(std::fs::metadata(&path).unwrap().len(), MAX_FILE + 1);
}

#[test]
fn trimming_keeps_the_first_prompt_and_records_what_went() {
    let dir = tempfile::tempdir().unwrap();
    let id = ThreadId::random();
    let mut writer = Writer::open(dir.path(), &id).unwrap();
    let notice = Entry {
        id: EntryId(1),
        kind: EntryKind::Notice {
            answers: Vec::new(),
            details: None,
            level: NoticeLevel::Info,
            text: "started".to_owned(),
        },
    };
    let mut entries = vec![notice, user(2, "the first prompt")];
    entries.extend((3..=20).map(|id| agent(id, &"x".repeat(100))));
    writer.append(&entries.iter().collect::<Vec<_>>()).unwrap();
    // Room for the first prompt and about five replies.
    writer.trim_to(800).unwrap();
    let contents = Contents::read(writer.path()).unwrap();
    assert_eq!(contents.entries[0], user(2, "the first prompt"));
    let kept = ids(&contents);
    assert_eq!(*kept.last().unwrap(), 20);
    assert!(kept.len() < 10, "{kept:?}");
    let first_reply = kept[1];
    assert_eq!(
        contents.trimmed,
        [
            Trimmed {
                from: EntryId(1),
                to: EntryId(1)
            },
            Trimmed {
                from: EntryId(3),
                to: EntryId(first_reply - 1)
            },
        ]
    );
    assert!(contents.is_trimmed(EntryId(1)) && contents.is_trimmed(EntryId(3)));
    assert!(!contents.is_trimmed(EntryId(2)));
    assert_eq!(contents.last_id(), Some(EntryId(20)));
    // Writing a trimmed entry again does not bring it back, and a second
    // trim extends the range rather than adding one.
    writer.append(&[&agent(3, "late update")]).unwrap();
    writer.trim_to(500).unwrap();
    let contents = Contents::read(writer.path()).unwrap();
    assert!(!ids(&contents).contains(&3));
    assert_eq!(contents.trimmed.len(), 2);
    assert_eq!(contents.trimmed[1].from, EntryId(3));
}

#[test]
fn an_entry_too_large_to_keep_is_recorded_as_trimmed() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = Writer::open(dir.path(), &ThreadId::random()).unwrap();
    let huge = agent(2, &"x".repeat(MAX_LINE));
    writer
        .append(&[&user(1, "hi"), &huge, &agent(3, "ok")])
        .unwrap();
    let contents = Contents::read(writer.path()).unwrap();
    assert_eq!(ids(&contents), [1, 3]);
    assert!(contents.is_trimmed(EntryId(2)));
}

#[test]
fn a_file_past_its_bound_does_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = transcript_path(dir.path(), &ThreadId::random());
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_FILE + 1).unwrap();
    assert!(matches!(Contents::read(&path), Err(Error::TranscriptSize)));
}
