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
fn only_long_text_attaches_as_a_file() {
    assert!(!is_long_text("short"));
    assert!(!is_long_text(&"a\n".repeat(LONG_TEXT_LINES)));
    assert!(is_long_text(&"a\n".repeat(LONG_TEXT_LINES + 1)));
    assert!(!is_long_text(&"x".repeat(LONG_TEXT_CHARS - 1)));
    assert!(is_long_text(&"x".repeat(LONG_TEXT_CHARS)));
    // Characters, not bytes, count.
    assert!(!is_long_text(&"é".repeat(LONG_TEXT_CHARS - 1)));
}
