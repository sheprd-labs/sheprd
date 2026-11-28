//! Chats let go for room, and chats off screen, show at once.

use super::*;

#[test]
fn a_chat_let_go_for_room_shows_at_once_when_it_comes_back() {
    let mut harness = Harness::new();
    let (one, two) = (target("p1", A), target("p2", B));
    let known = vec![one.clone(), two.clone()];
    let within = |harness: &mut Harness, targets: Vec<Target>| {
        let path = harness.program.path.clone();
        let sinces = &mut harness.sinces;
        harness
            .chats
            .reconcile_within(targets, &known, harness.now, &mut |_, since, resend| {
                sinces.push(since);
                Link::start(path.clone(), since, resend)
            });
    };
    within(&mut harness, vec![one.clone()]);
    let (mut stream, _) = harness.program.accept();
    send(&mut stream, &welcome());
    send(&mut stream, &snapshot(cursor(1, 0), vec![agent(1, "kept")]));
    harness.poll_until(|chats| chats.pane("p1").is_some_and(|pane| pane.attached()));

    within(&mut harness, vec![two.clone()]);
    assert!(harness.chats.pane("p1").is_none());
    assert!(harness.chats.resting("p1"));
    // Its connection went with it, as one let go always did.
    assert_eq!(read(&mut stream), None);
    let _two = harness.program.accept();

    within(&mut harness, vec![one]);
    assert!(!harness.chats.resting("p1"));
    // Shown with what it had before its program answers again.
    assert!(harness.chats.shown("p1"));
    assert_eq!(text(&harness.chats), ["kept"]);
    assert_eq!(harness.chats.pane("p1").unwrap().list.item_count(), 1);
    // It asks for the whole transcript rather than a replay.
    let (_again, greeting) = harness.program.accept();
    assert_eq!(greeting, hello(None));
    assert_eq!(harness.sinces.last(), Some(&None));

    // A chat whose program ended does not rest.
    within(&mut harness, vec![two]);
    assert!(harness.chats.resting("p1"));
    harness.reconcile(Vec::new());
    assert!(!harness.chats.resting("p1"));
}

#[test]
fn a_chat_with_nothing_to_show_does_not_rest() {
    let mut harness = Harness::new();
    let (one, two) = (target("p1", A), target("p2", B));
    let known = vec![one.clone(), two.clone()];
    let path = harness.program.path.clone();
    let mut connect = |_: &Target, since, resend| Link::start(path.clone(), since, resend);
    let now = harness.now;
    harness
        .chats
        .reconcile_within(vec![one], &known, now, &mut connect);
    harness
        .chats
        .reconcile_within(vec![two], &known, now, &mut connect);
    assert!(!harness.chats.resting("p1"));
}

#[test]
fn a_chat_off_screen_catches_up_its_view_once_it_shows() {
    let mut harness = Harness::new();
    let mut stream = harness.attach(cursor(1, 0), vec![agent(1, "a")]);
    send(&mut stream, &change(append(1, "b"), cursor(1, 1)));
    send(
        &mut stream,
        &change(
            Change::Push {
                entry: agent(2, "c"),
            },
            cursor(1, 2),
        ),
    );
    let now = harness.now;
    wait_until(|| {
        harness.chats.poll(now, |_| false);
        harness
            .chats
            .pane("p1")
            .unwrap()
            .session
            .transcript()
            .entries
            .len()
            == 2
    });
    // The transcript follows; the list waits until the chat shows.
    let pane = harness.chats.pane("p1").unwrap();
    assert_eq!(pane.list.item_count(), 1);
    assert!(harness.chats.poll(now, |_| true));
    let pane = harness.chats.pane("p1").unwrap();
    assert_eq!(pane.list.item_count(), 2);
    assert_eq!(text(&harness.chats), ["ab", "c"]);
}
