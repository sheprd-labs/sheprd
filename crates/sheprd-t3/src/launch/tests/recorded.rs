//! The project and threads a recorded session started on a T3 Code 0.1.9
//! server, rebuilt from what `sheprd-t3 new` is asked.

use std::{net::TcpListener, thread};

use super::*;
use tungstenite::Message;

/// The client request `id` of the recording.
fn request(id: &str) -> Value {
    frames(LAUNCHES)
        .into_iter()
        .find(|frame| frame["dir"] == "client" && frame["id"] == id)
        .unwrap()
}

/// The server's answer to request `id`, as recorded.
fn answer(text: &str, id: &str) -> Value {
    frames(text)
        .into_iter()
        .find(|frame| frame["_tag"] == "Exit" && frame["requestId"] == id)
        .map(|mut frame| {
            frame.as_object_mut().unwrap().remove("dir");
            frame
        })
        .unwrap()
}

fn recorded_id(payload: &Value) -> Uuid {
    let command = payload["commandId"].as_str().unwrap();
    command.strip_prefix("sheprd:").unwrap().parse().unwrap()
}

fn repo() -> Checkout {
    Checkout {
        root: PathBuf::from("/tmp/t3rec/work/repo"),
        worktree: None,
    }
}

fn fake() -> ModelSelection {
    ModelSelection {
        instance_id: "fakeacp".to_owned(),
        model: "fast".to_owned(),
    }
}

#[test]
fn each_recorded_launch_is_the_one_a_request_makes() {
    let project = "2603d1f5-162c-463b-a960-64ddc3ec905d";
    let base = Request {
        runtime_mode: RuntimeMode::ApprovalRequired,
        ..Request::default()
    };
    // In the repository, with a prompt and a title T3 may replace.
    let payload = request("5")["payload"].clone();
    let asked = Request {
        prompt: Some("hello from sheprd".to_owned()),
        ..base.clone()
    };
    let mut built = serde_json::to_value(launch(
        &asked,
        &repo(),
        project,
        fake(),
        recorded_id(&payload),
    ))
    .unwrap();
    built["title"] = "Say hello".into();
    assert_eq!(built, payload);
    // In a worktree that exists, with no prompt.
    let payload = request("7")["payload"].clone();
    let existing = Checkout {
        worktree: Some(PathBuf::from("/tmp/t3rec/work/repo-wt")),
        ..repo()
    };
    let asked = Request {
        title: Some("Existing worktree".to_owned()),
        ..base.clone()
    };
    let built = launch(&asked, &existing, project, fake(), recorded_id(&payload));
    assert_eq!(serde_json::to_value(built).unwrap(), payload);
    // In a new worktree T3 makes.
    let payload = request("8")["payload"].clone();
    let asked = Request {
        prompt: Some("hello from a worktree".to_owned()),
        title: Some("New worktree".to_owned()),
        worktree: Some(NewWorktree {
            base: "main".to_owned(),
            branch: Some("t3-new".to_owned()),
        }),
        ..base
    };
    let built = launch(&asked, &repo(), project, fake(), recorded_id(&payload));
    assert_eq!(serde_json::to_value(built).unwrap(), payload);
}

#[test]
fn a_new_project_is_named_for_its_folder() {
    let mut recorded = request("2")["payload"].clone();
    let mut built = serde_json::to_value(create_project(&repo(), Uuid::nil())).unwrap();
    // The ids are the client's own choice.
    for payload in [&mut recorded, &mut built] {
        let fields = payload.as_object_mut().unwrap();
        fields.remove("commandId");
        fields.remove("projectId");
    }
    assert_eq!(built, recorded);
}

/// Serves `start` over a socket with the recorded answers: the agents, a
/// thread list with no project for the checkout, the project created, and
/// the thread launched; returns what the client asked for.
fn serve(listener: TcpListener) -> Vec<Value> {
    let (stream, _) = listener.accept().unwrap();
    let mut socket = tungstenite::accept(stream).unwrap();
    let mut asked = Vec::new();
    let shell = frames(LAUNCHES)
        .into_iter()
        .find(|frame| frame["_tag"] == "Chunk" && frame["requestId"] == "1")
        .unwrap();
    loop {
        let message = match socket.read() {
            Ok(Message::Text(text)) => text,
            Ok(_) => continue,
            Err(_) => return asked,
        };
        let frame: Value = serde_json::from_str(message.as_str()).unwrap();
        if frame["_tag"] != "Request" {
            continue;
        }
        let id = frame["id"].clone();
        let mut reply = match frame["tag"].as_str().unwrap() {
            "server.getConfig" => answer(STEER, "1"),
            "orchestration.subscribeShell" => {
                let mut chunk = shell.clone();
                chunk.as_object_mut().unwrap().remove("dir");
                chunk
            }
            MUTATE_PROJECT => answer(LAUNCHES, "2"),
            LAUNCH => answer(LAUNCHES, "5"),
            other => panic!("unexpected {other}"),
        };
        reply["requestId"] = id;
        socket
            .send(Message::Text(reply.to_string().into()))
            .unwrap();
        asked.push(frame);
        if asked.len() == 4 {
            let _ = socket.close(None);
        }
    }
}

#[test]
fn start_creates_the_project_then_launches_the_thread() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve(listener));
    let url = url::Url::parse(&format!("ws://127.0.0.1:{port}/ws")).unwrap();
    let mut connection = Connection::open(&url).unwrap();
    let request = Request {
        cwd: dir.path().to_owned(),
        prompt: Some("hi".to_owned()),
        ..Request::default()
    };
    let id = Uuid::from_u128(1);
    let thread = start(&mut connection, &request, id).unwrap();
    assert_eq!(
        thread,
        "thread:project:2603d1f5-162c-463b-a960-64ddc3ec905d:aec8b272-e187-4785-9dfe-0aa0f119d0e1"
    );
    connection.close();
    let asked = server.join().unwrap();
    let tags: Vec<&str> = asked
        .iter()
        .map(|frame| frame["tag"].as_str().unwrap())
        .collect();
    assert_eq!(
        tags,
        [
            "server.getConfig",
            "orchestration.subscribeShell",
            MUTATE_PROJECT,
            LAUNCH
        ]
    );
    let root = fs::canonicalize(dir.path()).unwrap();
    assert_eq!(
        asked[2]["payload"]["workspaceRoot"],
        root.to_string_lossy().as_ref()
    );
    // The thread runs in the project T3 answered with, on the first agent
    // that is ready.
    let launched = &asked[3]["payload"];
    assert_eq!(
        launched["projectId"],
        "2603d1f5-162c-463b-a960-64ddc3ec905d"
    );
    assert_eq!(
        launched["modelSelection"],
        json!({"instanceId": "fakeacp", "model": "fast"})
    );
    assert_eq!(launched["commandId"], format!("sheprd:{id}"));
    assert_eq!(launched["workspaceStrategy"], json!({"type": "root"}));
}

#[test]
fn a_project_another_client_just_created_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let theirs = json!({"kind": "snapshot", "snapshot": {"schemaVersion": 2,
        "snapshotSequence": 9, "threads": [], "projects": [{"id": "theirs",
            "title": "repo", "workspaceRoot": root.to_string_lossy()}]}});
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut shells = 0;
        let mut launched = None;
        while let Ok(message) = socket.read() {
            let Message::Text(text) = message else {
                continue;
            };
            let frame: Value = serde_json::from_str(text.as_str()).unwrap();
            if frame["_tag"] != "Request" {
                continue;
            }
            let mut reply = match frame["tag"].as_str().unwrap() {
                "server.getConfig" => answer(STEER, "1"),
                "orchestration.subscribeShell" => {
                    shells += 1;
                    // Empty at first; the other client's project after.
                    let values = if shells == 1 {
                        json!([{"kind": "snapshot", "snapshot": {"schemaVersion": 2,
                            "snapshotSequence": 8, "threads": [], "projects": []}}])
                    } else {
                        json!([theirs.clone()])
                    };
                    json!({"_tag": "Chunk", "values": values})
                }
                // The recorded refusal of a second project for one root.
                MUTATE_PROJECT => answer(LAUNCHES, "4"),
                LAUNCH => {
                    launched = Some(frame["payload"]["projectId"].clone());
                    answer(LAUNCHES, "5")
                }
                other => panic!("unexpected {other}"),
            };
            reply["requestId"] = frame["id"].clone();
            socket
                .send(Message::Text(reply.to_string().into()))
                .unwrap();
        }
        launched
    });
    let url = url::Url::parse(&format!("ws://127.0.0.1:{port}/ws")).unwrap();
    let mut connection = Connection::open(&url).unwrap();
    let request = Request {
        cwd: dir.path().to_owned(),
        ..Request::default()
    };
    start(&mut connection, &request, Uuid::from_u128(2)).unwrap();
    connection.close();
    assert_eq!(server.join().unwrap(), Some(json!("theirs")));
}
