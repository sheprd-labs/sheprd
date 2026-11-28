//! A Luau screen of the phone's app changed from this computer while it
//! runs (`sheprd phone luau`): the files an agent edits go to the phone,
//! which lays them over the screen's bundled ones and reloads, keeping its
//! state, and says what loading them found. Only a debug build of the app
//! takes them; a build from the App Store runs its bundled Luau alone.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The most bytes of Luau one push carries, all files together as the
/// JSON that carries them, escapes and all: a push stays well within the
/// 256 KiB a command's request may take on Sheprd's control socket.
pub const MAX_LUAU_BYTES: usize = 192 * 1024;
/// The most files one push carries.
pub const MAX_LUAU_FILES: usize = 64;
/// The longest a Luau screen's name may be, in bytes.
pub const MAX_SCREEN_BYTES: usize = 64;
/// The most lines of each kind a phone reports, so its answer fits the
/// live socket.
pub const MAX_REPORT_LINES: usize = 12;

/// Whether `name` may name a Luau screen, as its folder is named in the
/// app's bundle (`pull-requests`).
pub fn valid_screen(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SCREEN_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Whether `path` may name one of a Luau screen's files: a `.luau` file
/// relative to the screen's folder, as `main.luau`, `style.luau`,
/// `components/row.luau` or `mods/status/main.luau`, never out of it.
pub fn valid_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    path.ends_with(".luau")
        && path.len() <= 200
        && parts.len() <= 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

/// Whether `files` may be pushed: within the limits, each named by a
/// [`valid_path`].
pub fn valid_files(files: &BTreeMap<String, String>) -> bool {
    files.len() <= MAX_LUAU_FILES
        && files.keys().all(|path| valid_path(path))
        && encoded_len(files).is_some_and(|len| len <= MAX_LUAU_BYTES)
}

/// How many bytes `files` take as JSON, counted without writing them out.
fn encoded_len(files: &BTreeMap<String, String>) -> Option<usize> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, files).ok()?;
    Some(count.0)
}

/// What the phone found loading a push, each list at most
/// [`MAX_REPORT_LINES`] lines.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LuauReport {
    /// Whether the screen is open, so it redrew with the push; else the
    /// files were checked without drawing and wait for it to open.
    #[serde(default)]
    pub shown: bool,
    /// What failed to load or draw, by file: the screen keeps the last
    /// version that worked meanwhile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    /// What draws as a placeholder: a component or element the app lacks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// The last lines the screen's mods logged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_is_measured_as_the_json_that_carries_it() {
        // Escapes double a backslash's size: raw, this fits; as JSON, not.
        let text = "\\".repeat(MAX_LUAU_BYTES * 3 / 4);
        assert!(text.len() < MAX_LUAU_BYTES);
        let files = BTreeMap::from([("main.luau".to_owned(), text)]);
        assert!(!valid_files(&files));

        let files = BTreeMap::from([("main.luau".to_owned(), "x".repeat(MAX_LUAU_BYTES / 2))]);
        assert!(valid_files(&files));
    }

    #[test]
    fn the_largest_push_fits_a_control_request() {
        let files = BTreeMap::from([("main.luau".to_owned(), "x".repeat(MAX_LUAU_BYTES - 64))]);
        assert!(valid_files(&files));
        // What the request adds around the files: the command, the screen
        // and the device asked for.
        let envelope = 4 * 1024;
        assert!(encoded_len(&files).unwrap_or(usize::MAX) + envelope <= 256 * 1024);
    }
}
