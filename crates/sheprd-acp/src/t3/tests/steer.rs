use std::net::TcpListener;

use super::*;
use serde_json::{Value, json};
use sheprd_chat_protocol::ServerMessage;
use tungstenite::Message;

fn read(socket: &mut tungstenite::WebSocket<std::net::TcpStream>) -> Value {
    loop {
        if let Message::Text(text) = socket.read().unwrap() {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

fn send(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, frame: &Value) {
    socket
        .send(Message::Text(frame.to_string().into()))
        .unwrap();
}

fn order(command: Command, n: u128) -> Order {
    Order {
        command,
        conn: ConnId(1),
        id: CommandId(uuid::Uuid::from_u128(n)),
    }
}

#[test]
fn a_clients_commands_reach_t3_and_its_answers_come_back_as_acks() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        send(
            &mut socket,
            &json!({"_tag": "Chunk", "requestId": "2", "values": [{"kind": "snapshot",
                "snapshotSequence": 1, "projection": {
                    "thread": {"id": "thread-1", "title": "Steer"},
                    "runs": [{"id": "run-1", "ordinal": 1, "status": "running"}]}}]}),
        );
        assert_eq!(read(&mut socket), json!({"_tag": "Ack", "requestId": "2"}));
        // The stop, as T3's own stop button sends it.
        let stop = read(&mut socket);
        assert_eq!(stop["tag"], "orchestration.dispatchCommand");
        assert_eq!(
            stop["payload"],
            json!({"type": "run.interrupt", "threadId": "thread-1", "runId": "run-1",
                "holdQueue": true,
                "commandId": "sheprd:00000000-0000-0000-0000-000000000001"})
        );
        send(
            &mut socket,
            &json!({"_tag": "Exit", "requestId": stop["id"],
                "exit": {"_tag": "Success", "value": {"sequence": 2}}}),
        );
        // The run the stop ended, which the next command waits to see.
        send(
            &mut socket,
            &json!({"_tag": "Chunk", "requestId": "2", "values": [{"kind": "event",
                "sequence": 2, "event": {"type": "run.updated",
                    "payload": {"id": "run-1", "ordinal": 1, "status": "interrupted"}}}]}),
        );
        assert_eq!(read(&mut socket), json!({"_tag": "Ack", "requestId": "2"}));
        // A prompt T3 refuses.
        let prompt = read(&mut socket);
        assert_eq!(prompt["payload"]["type"], "message.dispatch");
        assert_eq!(prompt["payload"]["text"], "hi");
        // The run had ended, so it starts at once.
        assert_eq!(
            prompt["payload"]["dispatchMode"],
            json!({"type": "start_immediately"})
        );
        send(
            &mut socket,
            &json!({"_tag": "Exit", "requestId": prompt["id"], "exit": {"_tag": "Failure",
                "cause": [{"_tag": "Fail", "error": "busy"}]}}),
        );
        // One more that T3 never answers before the socket closes.
        let unanswered = read(&mut socket);
        assert_eq!(unanswered["payload"]["text"], "again");
        let _ = socket.close(None);
        while socket.read().is_ok() {}
    });
    let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let url = url::Url::parse(&format!("ws://127.0.0.1:{port}/ws")).unwrap();
    let connection = Connection::handshake(&url, stream).unwrap();
    // Through the connection's own stream, as `run` sets it: on Windows a
    // cloned socket is another handle.
    connection
        .stream()
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let (updates, changes) = bounded(1);
    let (orders, queue) = bounded(4);
    let worker = thread::spawn(move || {
        follow(
            connection,
            Follower::new("2".to_owned(), Vec::new()),
            &updates,
            &queue,
            Duration::from_secs(60),
        );
    });
    let next = || changes.recv_timeout(Duration::from_secs(10)).unwrap();
    let Step::Changed(transcript) = next() else {
        panic!("the snapshot first");
    };
    assert_eq!(transcript.status, Status::Working);
    assert!(!transcript.read_only);

    // A request that is not waiting is refused without reaching T3.
    let stale = Command::Answer {
        option: "accept".to_owned(),
        permission: sheprd_chat_protocol::PermissionId(5),
    };
    orders.send(order(stale, 9)).unwrap();
    assert_eq!(
        next(),
        Step::Acked {
            conn: ConnId(1),
            id: CommandId(uuid::Uuid::from_u128(9)),
            rejected: Some(Rejection::UnknownPermission),
        }
    );
    orders.send(order(Command::Cancel, 1)).unwrap();
    assert_eq!(
        next(),
        Step::Acked {
            conn: ConnId(1),
            id: CommandId(uuid::Uuid::from_u128(1)),
            rejected: None,
        }
    );
    let prompt = Command::Prompt {
        text: "hi".to_owned(),
        attachments: Vec::new(),
        origin: None,
    };
    orders.send(order(prompt, 2)).unwrap();
    let Step::Changed(stopped) = next() else {
        panic!("the stopped run");
    };
    assert_eq!(stopped.status, Status::Idle);
    assert_eq!(
        next(),
        Step::Acked {
            conn: ConnId(1),
            id: CommandId(uuid::Uuid::from_u128(2)),
            rejected: Some(Rejection::AgentUnavailable),
        }
    );
    let again = Command::Prompt {
        text: "again".to_owned(),
        attachments: Vec::new(),
        origin: None,
    };
    orders.send(order(again, 3)).unwrap();
    // A command T3 never answered is refused before the end is told.
    assert_eq!(
        next(),
        Step::Acked {
            conn: ConnId(1),
            id: CommandId(uuid::Uuid::from_u128(3)),
            rejected: Some(Rejection::AgentUnavailable),
        }
    );
    assert!(matches!(next(), Step::Ended(_)));
    // The worker's end drops the connection, closing the socket the server
    // reads until it closes.
    worker.join().unwrap();
    server.join().unwrap();
}

#[test]
fn a_command_before_the_snapshot_is_not_ready() {
    let follower = Follower::new("2".to_owned(), Vec::new());
    assert_eq!(
        follower.verdict(&order(Command::Cancel, 1)),
        Verdict::Answer(Some(Rejection::NotReady))
    );
}

fn chunk(values: Value) -> ServerFrame {
    ServerFrame::Chunk {
        request_id: "2".to_owned(),
        values: values.as_array().unwrap().clone(),
    }
}

fn exit(request: &str, sequence: u64) -> ServerFrame {
    serde_json::from_value(json!({"_tag": "Exit", "requestId": request,
        "exit": {"_tag": "Success", "value": {"sequence": sequence}}}))
    .unwrap()
}

fn idle() -> Follower {
    let mut follower = Follower::new("2".to_owned(), Vec::new());
    let _ = follower.frame(chunk(json!([{"kind": "snapshot", "snapshotSequence": 1,
        "projection": {"thread": {"id": "thread-1"}}}])));
    follower
}

#[test]
fn a_stop_right_after_a_prompt_waits_for_the_run_it_started() {
    let mut follower = idle();
    let now = Instant::now();
    assert!(follower.settled(now));
    let prompt = order(
        Command::Prompt {
            text: "hi".to_owned(),
            attachments: Vec::new(),
            origin: None,
        },
        1,
    );
    assert!(matches!(follower.verdict(&prompt), Verdict::Send(_)));
    follower.sent("10".to_owned(), &prompt);
    // Until T3 answers, the stop waits.
    assert!(!follower.settled(now));
    assert!(matches!(
        follower.frame(exit("10", 5)),
        Step::Acked { rejected: None, .. }
    ));
    // T3 answered before the thread shows the run: still waiting.
    assert!(!follower.settled(Instant::now()));
    let _ = follower.frame(chunk(json!([{"kind": "event", "sequence": 5, "event": {
        "type": "run.created", "payload": {"id": "run-1", "ordinal": 1, "status": "starting"}}}])));
    assert!(follower.settled(Instant::now()));
    let Verdict::Send(stop) = follower.verdict(&order(Command::Cancel, 2)) else {
        panic!("the stop goes to T3");
    };
    assert_eq!(serde_json::to_value(stop).unwrap()["runId"], "run-1");
}

#[test]
fn a_thread_that_never_shows_the_effect_holds_commands_only_briefly() {
    let mut follower = idle();
    let prompt = order(Command::Cancel, 1);
    follower.sent("10".to_owned(), &prompt);
    let _ = follower.frame(exit("10", 99));
    assert!(!follower.settled(Instant::now()));
    assert!(follower.settled(Instant::now() + SETTLE));
}

#[test]
fn a_command_sent_again_hears_t3s_first_answer() {
    let mut follower = idle();
    let _ = follower.frame(chunk(json!([{"kind": "event", "sequence": 2, "event": {
        "type": "run.created", "payload": {"id": "run-1", "ordinal": 1, "status": "running"}}}])));
    let stop = order(Command::Cancel, 7);
    assert!(matches!(follower.verdict(&stop), Verdict::Send(_)));
    follower.sent("10".to_owned(), &stop);
    let _ = follower.frame(exit("10", 3));
    let _ = follower.frame(chunk(json!([{"kind": "event", "sequence": 3, "event": {
        "type": "run.updated", "payload": {"id": "run-1", "ordinal": 1, "status": "interrupted"}}}])));
    // Nothing runs now, yet the same stop again was taken.
    assert_eq!(follower.verdict(&stop), Verdict::Answer(None));
    assert_eq!(
        follower.verdict(&order(Command::Cancel, 8)),
        Verdict::Answer(Some(Rejection::NotWorking))
    );
}

#[test]
fn an_attached_client_sees_a_request_asked_then_answered() {
    let mut follower = idle();
    let Step::Changed(quiet) = follower.frame(chunk(json!([{"kind": "event", "sequence": 2,
        "event": {"type": "turn-item.updated", "payload": {"id": "item-0", "ordinal": 0,
            "type": "system_notice", "message": "started"}}}])))
    else {
        panic!("a change");
    };
    let mut mirror = Mirror::new(*quiet, None);
    let (outbox, queue) = bounded(16);
    mirror.input(Input::Attach(crate::session::Attach {
        conn: ConnId(1),
        local: false,
        outbox,
        since: None,
    }));
    let mut client = match (queue.recv().unwrap(), queue.recv().unwrap()) {
        (ServerMessage::Welcome { .. }, ServerMessage::Snapshot { transcript, .. }) => *transcript,
        other => panic!("{other:?}"),
    };
    let mut apply = |mirror: &mut Mirror, follower: &mut Follower, values: Value| {
        if let Step::Changed(transcript) = follower.frame(chunk(values)) {
            mirror.update(*transcript);
        }
        while let Ok(message) = queue.try_recv() {
            let ServerMessage::Change { change, .. } = message else {
                panic!("a change, not {message:?}");
            };
            client.apply(&change).unwrap();
        }
        client.permissions.len()
    };
    let asked = apply(
        &mut mirror,
        &mut follower,
        json!([
            {"kind": "event", "sequence": 3, "event": {"type": "runtime-request.updated",
                "payload": {"id": "request-1", "status": "pending"}}},
            {"kind": "event", "sequence": 4, "event": {"type": "turn-item.updated",
                "payload": {"id": "item-1", "ordinal": 1, "type": "approval_request",
                    "requestId": "request-1", "requestKind": "command", "prompt": "Run it?"}}},
        ]),
    );
    assert_eq!(asked, 1);
    let answered = apply(
        &mut mirror,
        &mut follower,
        json!([
            {"kind": "event", "sequence": 5, "event": {"type": "runtime-request.updated",
                "payload": {"id": "request-1", "status": "resolved"}}},
        ]),
    );
    assert_eq!(answered, 0);
}
