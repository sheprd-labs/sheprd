//! Links an agent writes in its chat to a screen of its own computer, so the
//! person opens a workspace, worktree, tab, or agent it just made with a
//! click: `sheprd://workspace/WORKSPACE`, `sheprd://changes/WORKSPACE`,
//! `sheprd://chat/PANE`, or `sheprd://term/PANE`, with Herdr's ids. The
//! iPhone app reads the same links (`ScreenLink.swift`). A link names no
//! computer: it opens on the one whose chat shows it.

use crate::{NavigationTarget, OwnedNavigationTarget};
use url::Url;

pub(crate) const SCHEME: &str = "sheprd";

/// Where `destination` points, or `None` for any other link. A chat or a
/// terminal is its pane, which the window shows as it does: the chat over
/// it when there is one. Changes go to their workspace.
pub(crate) fn target(destination: &str) -> Option<OwnedNavigationTarget> {
    let url = Url::parse(destination).ok()?;
    if url.scheme() != SCHEME {
        return None;
    }
    let kind = url.host_str()?.to_ascii_lowercase();
    let mut segments = url.path_segments()?.filter(|segment| !segment.is_empty());
    let id = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    let id = percent_encoding::percent_decode_str(id)
        .decode_utf8()
        .ok()?
        .into_owned();
    match kind.as_str() {
        "chat" | "term" => Some(NavigationTarget::Pane(id)),
        "workspace" | "changes" => Some(NavigationTarget::Workspace(id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_kind() {
        let pane = |id: &str| Some(NavigationTarget::Pane(id.to_owned()));
        let workspace = |id: &str| Some(NavigationTarget::Workspace(id.to_owned()));
        assert_eq!(target("sheprd://chat/wF9:p1"), pane("wF9:p1"));
        assert_eq!(target("sheprd://term/wF9:p2"), pane("wF9:p2"));
        assert_eq!(target("sheprd://workspace/wF9"), workspace("wF9"));
        assert_eq!(target("sheprd://changes/wF9"), workspace("wF9"));
        assert_eq!(target("SHEPRD://Workspace/wF9"), workspace("wF9"));
        assert_eq!(target("sheprd://term/w%201"), pane("w 1"));
    }

    #[test]
    fn refuses_anything_else() {
        for text in [
            "https://x.dev/workspace/wF9",
            "herdr-remote://chat?id=1&pane=p",
            "sheprd://settings/push",
            "sheprd://workspace",
            "sheprd://workspace/",
            "sheprd://workspace/a/b",
            "/tmp/workspace/wF9",
        ] {
            assert_eq!(target(text), None, "{text}");
        }
    }
}
