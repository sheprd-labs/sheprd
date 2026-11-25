use super::*;

/// The guide's marker says how it opens: quietly for a machine made from an
/// image that may be set up, at once for one just installed.
#[test]
fn the_guide_marker_says_how_it_opens() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(GUIDE);
    assert_eq!(guide(&path).expect("absent"), None);
    fs::write(&path, b"").expect("write");
    assert_eq!(guide(&path).expect("installed"), Some(Opening::Shown));
    fs::write(&path, FROM_IMAGE).expect("write");
    assert_eq!(guide(&path).expect("from image"), Some(Opening::Quiet));
}
