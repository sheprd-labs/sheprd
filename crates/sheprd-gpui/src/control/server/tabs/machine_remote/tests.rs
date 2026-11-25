#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn the_lending_computer_hears_which_pane_asked() {
    let pane = Caller {
        pane_id: Some("w1-2".to_owned()),
        ..Caller::default()
    };
    assert_eq!(owner(&pane), "pane w1-2");
    let terminal = Caller {
        pane_id: Some("w1-2".to_owned()),
        terminal_id: Some("t7".to_owned()),
        ..Caller::default()
    };
    assert_eq!(owner(&terminal), "terminal t7");
    assert_eq!(owner(&Caller::default()), "command line");
}

#[test]
fn answers_say_which_computer_they_come_from() {
    let done = on_host("mini-2", Response::done("Starting default"));
    assert_eq!(done, Response::done("On mini-2: Starting default"));
    let refused = on_host(
        "mini-2",
        Response::error(ErrorCode::Busy, "default is busy"),
    );
    assert_eq!(
        refused,
        Response::error(ErrorCode::Busy, "On mini-2: default is busy")
    );
}

fn png(width: u32, height: u32) -> String {
    let screen = image::RgbImage::new(width, height);
    let mut bytes = std::io::Cursor::new(Vec::new());
    screen
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
}

#[test]
fn a_picture_is_written_only_when_it_is_the_png_it_says() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("shot.png");
    assert!(write_picture(&png(4, 3), 4, 3, &path).is_ok());
    assert!(path.is_file());
    let other = folder.path().join("other.png");
    assert!(matches!(
        write_picture(&png(4, 3), 8, 3, &other),
        Err(remote::Error::NotPng)
    ));
    let text = base64::engine::general_purpose::STANDARD.encode(b"not a png");
    assert!(matches!(
        write_picture(&text, 4, 3, &other),
        Err(remote::Error::NotPng)
    ));
    assert!(matches!(
        write_picture("%%%", 4, 3, &other),
        Err(remote::Error::Base64(_))
    ));
    assert!(!other.exists());
}

#[test]
fn every_forwarded_request_names_its_machine() {
    let name = || "default".to_owned();
    for op in [
        TabOp::Machine { name: name() },
        TabOp::MachineClaim { name: name() },
        TabOp::MachineStart { name: name() },
        TabOp::MachineScreenshot {
            name: name(),
            path: "/tmp/x.png".to_owned(),
        },
    ] {
        assert!(op.forwardable());
        assert_eq!(machine_name(&op), "default");
    }
}
