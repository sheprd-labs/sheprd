use super::Data;
use moonframe::moonframe_core::{Node, Roots, Workspace};

fn regions<'a>(node: &'a Node, names: &mut Vec<&'a str>) {
    match node {
        Node::View(view) if view.name == "sheprd" => {
            names.extend(view.props.get("region").and_then(Data::as_str));
        }
        Node::Stack(stack) => {
            for child in &stack.children {
                regions(child, names);
            }
        }
        _ => {}
    }
}

#[test]
fn the_stock_mods_mount_classic_controls_and_only_opt_in_to_the_editor() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("moonframe/app");
    let mut workspace = Workspace::open(Roots::new([root]), true);
    workspace.set_host("acp", Data::map([("available", true.into())]));
    for tools in [false, true] {
        workspace.set_host(
            "sheprd",
            Data::map([
                ("sidebar_title_height", 32_f64.into()),
                ("moonframe_tools", tools.into()),
                ("build", Data::map([("worktree", true.into())])),
            ]),
        );
        workspace.flush();
        let activity = if tools { "activity_editor" } else { "activity" };
        for (name, expected) in [
            ("header", &["header"][..]),
            ("activity", &[activity]),
            ("worktrees", &["sidebar_title", "worktrees"]),
            ("actions", &["sidebar_actions"]),
            ("agents", &["agents"]),
            ("devices", &["devices"]),
            ("status", &["status"]),
            ("build", &["build"]),
        ] {
            let slot = workspace
                .slot(name)
                .unwrap_or_else(|| panic!("missing {name}"));
            assert!(slot.error().is_none(), "{name}: {:?}", slot.error());
            let node = slot
                .view()
                .unwrap_or_else(|| panic!("missing view for {name}"));
            let mut found = Vec::new();
            regions(node, &mut found);
            assert_eq!(found, expected, "{name}");
        }
    }
}
