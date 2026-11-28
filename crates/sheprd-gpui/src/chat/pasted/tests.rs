use super::*;

#[test]
#[allow(clippy::unwrap_used)]
fn the_same_picture_is_saved_once_with_its_extension() {
    let bytes = b"not decoded here";
    let root = tempfile::tempdir().unwrap();
    let first = write(root.path(), "", "png", bytes).unwrap();
    let again = write(root.path(), "", "png", bytes).unwrap();
    assert_eq!(first, again);
    assert_eq!(first.extension().unwrap(), "png");
    assert_eq!(std::fs::read(&first).unwrap(), bytes);
}

#[test]
#[allow(clippy::unwrap_used)]
fn pasted_text_is_saved_as_a_named_txt_file() {
    let root = tempfile::tempdir().unwrap();
    let file = write(root.path(), "pasted-", "txt", b"line one\nline two\n").unwrap();
    assert_eq!(file.extension().unwrap(), "txt");
    let name = file.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with("pasted-"), "{name}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "line one\nline two\n"
    );
}

#[test]
fn only_text_meeting_the_paste_rule_attaches_as_a_file() {
    let mut chat = crate::config::chat::ChatConfig {
        long_paste_chars: 20,
        long_paste_lines: 3,
        ..Default::default()
    };
    assert!(!is_long_text("short", &chat));
    assert!(!is_long_text("a\nb\n", &chat));
    assert!(is_long_text("a\nb\nc", &chat));
    assert!(!is_long_text(&"é".repeat(19), &chat));
    assert!(is_long_text(&"é".repeat(20), &chat));
    chat.long_paste_as_file = false;
    assert!(!is_long_text(&"x\n".repeat(500), &chat));
}
