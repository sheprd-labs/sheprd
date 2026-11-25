//! The automatic name of a root space, following Herdr's folder naming.

use crate::{FolderStyle, Workspace};

impl Workspace {
    /// Follow the first terminal's checkout root, or its current directory
    /// outside Git. Explicit names and linked worktree rows remain stable.
    /// The caller resolves the folder on the terminal's host, off the UI thread.
    pub fn label_for_folder(&self, folder: &str, home: &str, style: FolderStyle) -> String {
        if self.custom_label || self.worktree.as_ref().is_some_and(|tree| tree.linked) {
            return self.label.clone();
        }
        folder_name(folder, home, style)
    }
}

/// Paths belong to the terminal's host, which can use either separator.
fn folder_name(folder: &str, home: &str, style: FolderStyle) -> String {
    let separator = |c| c == '/' || (style == FolderStyle::Windows && c == '\\');
    let path = folder.trim_end_matches(separator);
    let home = home.trim_end_matches(separator);
    let at_home = if style == FolderStyle::Windows {
        path.eq_ignore_ascii_case(home)
    } else {
        path == home
    };
    if folder.is_empty() || at_home {
        return "~".to_owned();
    }
    if path.is_empty() {
        return folder[..1].to_owned();
    }
    path.rsplit(separator)
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use crate::{FolderStyle, session::names::folder_name};

    #[test]
    fn folder_names_match_herdr_on_both_path_styles() {
        for (folder, home, expected) in [
            ("", "/Users/me", "~"),
            ("/Users/me", "/Users/me/", "~"),
            ("/", "/Users/me", "/"),
            ("/src/herdr-app/", "/Users/me", "herdr-app"),
            (r"C:\Users\me\", r"C:\Users\me", "~"),
            (r"C:\src\herdr-app\", r"C:\Users\me", "herdr-app"),
        ] {
            assert_eq!(
                folder_name(
                    folder,
                    home,
                    if folder.starts_with("C:") {
                        FolderStyle::Windows
                    } else {
                        FolderStyle::Posix
                    }
                ),
                expected
            );
        }
    }

    #[test]
    fn only_automatic_root_names_follow_the_resolved_folder() {
        use crate::{CoreWorkspace, Session, Worktree};
        for custom_label in [false, true] {
            for linked in [false, true] {
                let session = Session::core(
                    [CoreWorkspace {
                        branch: Some("main"),
                        cwd: "/original",
                        custom_label,
                        id: "root",
                        label: "original",
                        worktree: Some(Worktree {
                            key: "/original/.git".into(),
                            label: "original".into(),
                            linked,
                        }),
                    }],
                    [],
                    None,
                    |_| None,
                );
                let root = &session.workspaces[0];
                for (folder, expected) in [
                    ("/code/herdr-app", "herdr-app"),
                    ("/tmp/notes", "notes"),
                    ("/Users/me", "~"),
                ] {
                    assert_eq!(
                        root.label_for_folder(folder, "/Users/me", FolderStyle::Posix),
                        if custom_label || linked {
                            "original"
                        } else {
                            expected
                        }
                    );
                }
            }
        }
    }
    #[test]
    fn remote_posix_paths_keep_case_and_literal_backslashes() {
        assert_eq!(
            folder_name("/home/Alice", "/home/alice", FolderStyle::Posix),
            "Alice"
        );
        assert_eq!(
            folder_name(r"/tmp/a\b", "/home/alice", FolderStyle::Posix),
            r"a\b"
        );
        assert_eq!(
            folder_name(r"C:\Users\ALICE", r"c:\users\alice", FolderStyle::Windows),
            "~"
        );
    }
}
