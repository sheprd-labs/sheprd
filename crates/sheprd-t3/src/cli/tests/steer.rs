use super::*;

#[test]
fn send_takes_a_thread_and_its_text() {
    let send = parse(args(&[
        "send",
        "--token-file",
        "/t",
        "thread-1",
        "hello there",
    ]))
    .unwrap();
    assert_eq!(
        send.command,
        Command::Send {
            text: "hello there".to_owned(),
            thread: "thread-1".to_owned(),
        }
    );
    for wrong in [
        &["send", "--token-file", "/t", "thread-1"][..],
        &["send", "--token-file", "/t", "thread-1", "a", "b"],
    ] {
        assert_eq!(parse(args(wrong)), Err(Error::SendArguments));
    }
}

#[test]
fn stop_takes_a_thread() {
    let stop = parse(args(&["stop", "--json", "--token-file", "/t", "thread-1"])).unwrap();
    assert_eq!(
        stop.command,
        Command::Stop {
            thread: "thread-1".to_owned(),
        }
    );
    assert!(stop.json);
    assert_eq!(
        parse(args(&["stop", "--token-file", "/t"])),
        Err(Error::Arguments("stop"))
    );
    assert_eq!(parse(args(&["stop", "thread-1"])), Err(Error::TokenFile));
}

#[test]
fn text_after_a_double_dash_is_sent_as_written() {
    for text in ["- explain this change", "--help", "--json"] {
        let send = parse(args(&[
            "send",
            "--token-file",
            "/t",
            "thread-1",
            "--",
            text,
        ]))
        .unwrap();
        assert_eq!(
            send.command,
            Command::Send {
                text: text.to_owned(),
                thread: "thread-1".to_owned(),
            }
        );
        assert!(!send.json);
    }
}
