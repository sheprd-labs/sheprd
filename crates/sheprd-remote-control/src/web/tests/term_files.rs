use super::terminal::{authorized, with_terminal};
use super::*;
use crate::web::uploads::{Error, MAX_FILE, file_name};

#[test]
fn a_file_keeps_its_name_made_safe() {
    assert_eq!(file_name("report.pdf"), "report.pdf");
    assert_eq!(file_name("../../etc/passwd"), "passwd");
    assert_eq!(file_name(r"C:\Users\me\notes.txt"), "notes.txt");
    assert_eq!(file_name("my notes (1).md"), "my_notes__1_.md");
    assert_eq!(file_name(".env"), "env");
    assert_eq!(file_name(".."), "file");
    assert_eq!(file_name(""), "file");
    assert_eq!(file_name("$(rm -rf ~);.sh"), "__rm_-rf____.sh");
    let long = format!("{}.tar.gz", "a".repeat(300));
    let kept = file_name(&long);
    assert_eq!(kept.len(), 100);
    assert!(kept.ends_with(".gz"), "{kept}");
}

#[test]
fn a_name_in_letters_of_several_bytes_is_cut_by_bytes_at_a_letter() {
    // 80 letters of 3 bytes each: few characters, but 244 bytes.
    let long = format!("{}.pdf", "报".repeat(80));
    let kept = file_name(&long);
    assert!(kept.len() <= 100, "{} bytes", kept.len());
    assert!(kept.ends_with(".pdf"), "{kept}");
    assert_eq!(kept, format!("{}.pdf", "报".repeat(32)));
    // With the id and `-` before it, the stored name fits a file system.
    assert!(format!("{}-{kept}", "a".repeat(22)).len() <= 255);
    let dir = tempfile::tempdir().unwrap();
    let uploads = Uploads::new(dir.path());
    let id = uploads
        .store_file("abcdefghijklmnopqrstuv".into(), &long, b"x", 1_000)
        .unwrap();
    assert!(uploads.path(&id, 1_000).unwrap().exists());
}

#[test]
fn a_file_of_any_kind_is_stored_privately_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let uploads = Uploads::new(dir.path());
    let id = uploads
        .store_file("abc".into(), "data.csv", b"a,b\n1,2\n", 1_000)
        .unwrap();
    let path = uploads.path(&id, 1_000).unwrap();
    assert_eq!(path, dir.path().join("uploads").join("abc-data.csv"));
    assert_eq!(std::fs::read(&path).unwrap(), b"a,b\n1,2\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let huge = vec![0; MAX_FILE + 1];
    assert!(matches!(
        uploads.store_file("big".into(), "big.bin", &huge, 0),
        Err(Error::FileTooLarge)
    ));
}

#[tokio::test]
async fn files_reach_only_terminal_panes() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_terminal(dir.path());
    let (_, token) = signed_in(&state, 6);
    let file = |pane: &str, query: &str| {
        let request = HttpRequest::post(format!("/api/term/{pane}/file{query}"))
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(&b"%PDF-1.7"[..]))
            .unwrap();
        authorized(request, &token)
    };
    assert_eq!(
        send(&state, file("p9", "?name=a.pdf")).await.0,
        StatusCode::NOT_FOUND
    );
    // A file needs a name.
    assert_eq!(
        send(&state, file("t2-p1", "")).await.0,
        StatusCode::BAD_REQUEST
    );
    // Stored for a known pane, then its path goes to Herdr, which this
    // test has none of.
    let (status, _, _) = send(&state, file("t2-p1", "?name=..%2Fspec%20v2.pdf")).await;
    assert_ne!(status, StatusCode::OK);
    assert_ne!(status, StatusCode::BAD_REQUEST);
    let stored: Vec<_> = std::fs::read_dir(dir.path().join("uploads"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].ends_with("-spec_v2.pdf"), "{stored:?}");
}
