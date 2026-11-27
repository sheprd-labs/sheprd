//! Synthetic image and file conversations for native capture.

use anyhow::{Context as _, Result};
use sheprd_chat_protocol::{Entry, EntryId, EntryKind};

pub(super) fn entries() -> Result<Vec<Entry>> {
    use sheprd_chat_protocol::{Author, Image, NoticeLevel};
    // Synthetic 240×128 RGB checkerboard; no file, URL, or private screenshot.
    const CHECKERBOARD: &str = "iVBORw0KGgoAAAANSUhEUgAAAPAAAACACAIAAAC6FXMEAAAB6ElEQVR42u3SMQ0AIAxFwUpDG8IQws6OBFYMdGhzSQW8/F7sc1NvzJV6+vX/FwbVD7RB9QMNhH6ggdAPtH6gDaofaIPqBxoI/UADoR9o/UAbVD/QBtUPNBD6gQZCP9D6gTaofqANqh9oIPQDDYR+oPUDbVD9QAOhH2gg9AMNhH6g9QNtUP1AA6EfaCD0A61fP9D6gTao/gKgPUx/p36g9QNtUP1AA6EfaCD0A60faIPqB9qg+oEGQj/QQOgHWj/QBtUPtEH1Aw2EfqCB0A+0fqANqh9og+oHGgj9QAOhH2j9QBtUP9BA6AcaCP1AA6EfaP1AG1Q/0EDoBxoI/UDr1w+0fqANqr8CaA/T36kfaP1AG1Q/0EDoBxoI/UDrB9qg+oE2qH6ggdAPNBD6gdYPtEH1A21Q/UADoR9oIPQDrR9og+oH2qD6gQZCP9BA6AdaP9AG1Q80EPqBBkI/0EDoB1o/0AbVDzQQ+oEGQj/Q+vUDrR9og+qvANrD9HfqB1o/0AbVDzQQ+oEGQj/Q+oE2qH6gDaofaCD0Aw2EfqD1A21Q/UAbVD/QQOgHGgj9QOsH2qD6gTaofqCB0A80EPqB1g+0QfUDDYR+oIHQDzQQ+oHWD7RB9QMNhH6ggdAPtH79QOsH2qD6C/Q/rkEStgLPXQwAAAAASUVORK5CYII="; // i18n: data
    Ok(vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                level: NoticeLevel::Info,
                text: "Environment context updated".into(), // i18n: data
                details: Some("<environment_context>\n  <cwd>/tmp/herdr-rich-fixture</cwd>\n</environment_context>".into()),
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                level: NoticeLevel::Info,
                text: "Image attachment · Image #1".into(), // i18n: data
                details: Some("<image name=[Image #1] path=\"/tmp/herdr-rich-fixture/checkerboard.png\"></image>".into()), // i18n: data
            },
        },
        Entry {
            id: EntryId(3),
            kind: EntryKind::Image {
                author: Author::User,
                image: Image::new("image/png", CHECKERBOARD.into())
                    .context("synthetic attachment image is invalid")?,
                caption: None,
            },
        },
        Entry {
            id: EntryId(4),
            kind: EntryKind::User {
                text: "Can you explain this image and link the notes?".into(), // i18n: data
                files: 0,
                note: None,
            },
        },
        Entry {
            id: EntryId(5),
            kind: EntryKind::Agent {
                text: "The attachment shows a blue checkerboard.\n\n[Changes and verification details](/tmp/herdr-rich-fixture/notes.md)\n\nThe document opens only when you click its link.".into(), // i18n: data
            },
        },
    ])
}

pub(super) fn files() -> Result<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/attachments");
    let video = url::Url::from_file_path(root.join("Video walkthrough.mp4")) // i18n: data
        .map_err(|()| anyhow::anyhow!("video fixture path is not absolute"))?;
    let pdf = url::Url::from_file_path(root.join("Review report.pdf")) // i18n: data
        .map_err(|()| anyhow::anyhow!("PDF fixture path is not absolute"))?;
    Ok(format!(
        "Here are the video walkthrough and the PDF report. Open either attachment to view it in its usual app.\n\n[Video walkthrough.mp4]({video})\n\n[Review report.pdf]({pdf})" // i18n: data
    )) // i18n: data
}
