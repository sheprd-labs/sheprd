//! Load the real shell and check the ancestors of its native page region.
use moonframe::{
    Data,
    moonframe_core::{Node, Roots, Workspace},
};
use std::path::PathBuf;

fn contains_body(node: &Node) -> bool {
    let (found, background) = match node {
        Node::View(view) => (
            view.name == "sheprd" && view.id == "body",
            view.look.style.background,
        ),
        Node::Stack(stack) => (
            stack.children.iter().any(contains_body),
            stack.look.style.background,
        ),
        Node::Split(split) => {
            return split.panes.iter().any(|pane| {
                let found = pane.children.iter().any(contains_body);
                if found {
                    assert!(
                        pane.look.style.background.is_none(),
                        "pane covers native page"
                    );
                }
                found
            });
        }
        _ => return false,
    };
    if found {
        assert!(
            background.is_none(),
            "shell ancestor covers native page: {node:?}"
        );
    }
    found
}

#[test]
fn shell_leaves_native_pages_unpainted() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("moonframe/app");
    for dark in [false, true] {
        let mut workspace = Workspace::open(Roots::new([root.clone()]), dark);
        // Supply an actual opaque color so an accidental shell background
        // cannot disappear merely because its provider has not started.
        workspace.set_host(
            "sheprd_theme",
            Data::map([("background", "#101419".into())]),
        );
        assert!(
            workspace.shell().error().is_none(),
            "{:?}",
            workspace.shell().error()
        );
        assert!(workspace.shell().view().is_some_and(contains_body));
    }
}
