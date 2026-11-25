use super::*;

#[test]
fn a_relaunch_continues_the_chat_in_its_thread() {
    let mut saved = record();
    assert_eq!(
        sweep::launch_argv(&saved),
        [
            "/bin/sheprd-acp",
            "--resume",
            "exact-session",
            "codex",
            "--",
            "--model",
            "chosen"
        ]
    );
    let thread = sheprd_chat_protocol::thread::ThreadId::random().to_string();
    saved.thread = Some(thread.clone());
    assert_eq!(
        sweep::launch_argv(&saved)[..5],
        [
            "/bin/sheprd-acp".to_owned(),
            "--resume".to_owned(),
            "exact-session".to_owned(),
            "--thread".to_owned(),
            thread
        ]
    );
}

#[test]
fn a_record_from_before_threads_reads_and_a_bad_thread_is_left_out() {
    let mut value = serde_json::to_value(record()).unwrap();
    assert!(value.get("thread").is_none());
    value["thread"] = "not a thread".into();
    let read: Record = serde_json::from_value(value).unwrap();
    assert!(!sweep::launch_argv(&read).contains(&"--thread".to_owned()));
}
