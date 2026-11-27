//! A phone's scroll through the hub reaches a Herdr 2 program that reads
//! the wheel; a shell's terminal gets nothing, and its view scrolls instead.

use super::*;

fn hub(source: &Arc<Source>) -> Hub {
    let layout = source.poll().unwrap();
    let mut hub = Hub::offline_in(merge(&Layout::default(), &layout), None, None);
    hub.core = Some(source.clone());
    hub
}

#[tokio::test]
async fn a_scroll_reaches_a_program_reading_the_wheel() {
    let (source, fake) = fake_with_doc(false, Value::Null, vec![record("wheel")]);
    let hub = hub(&source);
    assert!(hub.wheel(&id("wheel"), 3).await.unwrap());
    // One notch at the middle of the 80 by 24 screen, in SGR.
    assert_eq!(
        fake.inputs.recv_timeout(Duration::from_secs(5)).unwrap(),
        b"\x1b[<64;41;13M"
    );
    assert!(hub.wheel(&id("wheel"), -3).await.unwrap());
    assert_eq!(
        fake.inputs.recv_timeout(Duration::from_secs(5)).unwrap(),
        b"\x1b[<65;41;13M"
    );
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_shell_gets_no_wheel_and_is_left_to_its_driver() {
    let (source, fake) = fake();
    let hub = hub(&source);
    assert!(!hub.wheel(&id("t1"), 3).await.unwrap());
    // It only watched: no input, and no driver's attach taken from the desktop.
    assert_eq!(
        fake.attaches.recv_timeout(Duration::from_secs(2)).unwrap()["observe"],
        true
    );
    drop((hub, source));
    fake.server.join().unwrap();
    assert!(fake.inputs.try_recv().is_err());
    assert!(fake.attaches.try_recv().is_err());
}
