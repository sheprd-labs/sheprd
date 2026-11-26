//! "New terminals use Herdr 2" lets a phone start a core where Herdr 1 is
//! installed: read from the user's overrides first, then Sheprd's file.

use crate::hub::wake::chosen;
use sheprd_backend::TerminalBackend;

#[test]
fn the_overrides_decide_then_the_settings_and_otherwise_herdr_1_stays() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("config-gpui.toml");
    let overrides = dir.path().join("config-gpui.local.toml");
    assert!(
        chosen(&settings) != TerminalBackend::Core,
        "nothing says so"
    );

    std::fs::write(&settings, "herdr_2_terminals = true\n").unwrap();
    assert!(chosen(&settings) == TerminalBackend::Core);
    std::fs::write(
        &overrides,
        "layout = \"compact\"\nherdr_2_terminals = false\n",
    )
    .unwrap();
    assert!(
        chosen(&settings) != TerminalBackend::Core,
        "the user's overrides win"
    );
    std::fs::write(&overrides, "herdr_2_terminals = true\n").unwrap();
    std::fs::write(&settings, "herdr_2_terminals = false\n").unwrap();
    assert!(chosen(&settings) == TerminalBackend::Core);

    std::fs::write(&overrides, "herdr_2_terminals = [\n").unwrap();
    assert!(
        chosen(&settings) != TerminalBackend::Core,
        "unreadable overrides leave the file"
    );
    std::fs::write(&overrides, "[remote]\nherdr_2_terminals = true\n").unwrap();
    assert!(
        chosen(&settings) != TerminalBackend::Core,
        "only the top-level setting counts"
    );
}

#[test]
fn the_phone_and_desktop_resolve_the_same_backend_setting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    std::fs::write(&path, "herdr_2_terminals=true\nterminal_backend='herdr'").unwrap();
    assert_eq!(chosen(&path), TerminalBackend::Herdr);
    std::fs::write(
        sheprd_client::settings_files::overrides_path(&path),
        "terminal_backend='tmux'",
    )
    .unwrap();
    assert_eq!(
        chosen(&path),
        if cfg!(feature = "tmux") {
            TerminalBackend::Tmux
        } else {
            TerminalBackend::Core
        }
    );
}

#[cfg(feature = "tmux")]
#[test]
fn scrollback_reads_overrides_then_settings_and_clamps() {
    use crate::hub::wake::{DEFAULT_SCROLLBACK, scrollback};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let overrides = sheprd_client::settings_files::overrides_path(&path);
    assert_eq!(scrollback(&path), DEFAULT_SCROLLBACK);

    std::fs::write(&path, "herdr_2_scrollback = 25000\n").unwrap();
    assert_eq!(scrollback(&path), 25_000);

    std::fs::write(&overrides, "herdr_2_scrollback = 500\n").unwrap();
    assert_eq!(scrollback(&path), 1_000);

    std::fs::write(&overrides, "herdr_2_scrollback = 999999\n").unwrap();
    assert_eq!(scrollback(&path), 100_000);

    std::fs::write(&overrides, "herdr_2_scrollback = -5\n").unwrap();
    assert_eq!(scrollback(&path), DEFAULT_SCROLLBACK);
}
