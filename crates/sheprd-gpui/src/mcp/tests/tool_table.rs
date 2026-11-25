use super::*;
use crate::cli::control::{
    BrowserCommand, CodeCommand, FanOut, FileAt, MemoryAdd, MemoryCommand, Placement, WaitFor,
};
use crate::mcp::tools::Effect;
use sheprd_memory::Kind;
use std::collections::HashSet;

/// Arguments filling every required property with a value its schema allows.
fn minimal(tool: &Tool) -> Value {
    let schema = &tool.input_schema;
    let mut arguments = Map::new();
    for name in schema["required"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let property = &schema["properties"][name];
        let value = if let Some(choices) = property["enum"].as_array() {
            choices[0].clone()
        } else if property["type"] == "integer" {
            Value::from(property["minimum"].as_u64().unwrap_or(0).max(1))
        } else {
            Value::from("x")
        };
        arguments.insert(name.into(), value);
    }
    Value::Object(arguments)
}

#[test]
fn tool_names_are_unique_and_valid_mcp_names() {
    let mut seen = HashSet::new();
    for spec in tools::all() {
        assert!(seen.insert(spec.name), "{} twice", spec.name);
        assert!(
            !spec.name.is_empty()
                && spec.name.len() <= 64
                && spec
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
            "{}",
            spec.name
        );
        assert!(tools::find(spec.name).is_some());
    }
    assert!(tools::find("no_such_tool").is_none());
}

#[test]
fn every_schema_is_a_closed_object_whose_required_arguments_exist() {
    for spec in tools::all() {
        let tool = spec.tool();
        let schema = &tool.input_schema;
        assert_eq!(schema["type"], "object", "{}", spec.name);
        assert_eq!(schema["additionalProperties"], false, "{}", spec.name);
        let properties = schema["properties"].as_object().unwrap();
        assert!(properties.contains_key("window"), "{}", spec.name);
        for required in spec.required {
            assert!(
                properties.contains_key(*required),
                "{}: {required}",
                spec.name
            );
        }
        assert!(!tool.description.is_empty() && tool.title.is_some());
    }
}

#[test]
fn every_tool_builds_from_its_required_arguments_alone() {
    for spec in tools::all() {
        let arguments = minimal(&spec.tool());
        assert!(
            build(spec.name, arguments.clone()).is_ok(),
            "{} with {arguments}",
            spec.name
        );
    }
}

#[test]
fn a_tool_missing_a_required_argument_is_refused() {
    for spec in tools::all() {
        for required in spec.required {
            let mut arguments = minimal(&spec.tool());
            arguments.as_object_mut().unwrap().remove(*required);
            assert!(
                build(spec.name, arguments).is_err(),
                "{} without {required}",
                spec.name
            );
        }
    }
}

#[test]
fn hints_follow_each_tools_effect() {
    for spec in tools::all() {
        let annotations = spec.tool().annotations.unwrap();
        assert_eq!(
            annotations.read_only_hint,
            spec.effect == Effect::Reads,
            "{}",
            spec.name
        );
        assert_eq!(
            annotations.destructive_hint,
            spec.effect == Effect::Removes,
            "{}",
            spec.name
        );
    }
    // What removes or moves work is marked so, though it only opens a dialog.
    for name in ["checkpoint_restore", "memory_forget", "teleport"] {
        assert_eq!(tools::find(name).unwrap().effect, Effect::Removes, "{name}");
    }
}

#[test]
fn tools_become_the_commands_they_are_named_after() {
    assert_eq!(
        build("memory_add", json!({ "text": "Use just ci", "kind": "command", "name": "ci", "details": "d", "branch": true, "pin": true })).unwrap(),
        ControlCommand::Memory(MemoryCommand::Add(MemoryAdd {
            branch: true,
            name: Some("ci".into()),
            kind: Kind::Command,
            text: "Use just ci".into(),
            details: Some("d".into()),
            pinned: true,
        }))
    );
    assert_eq!(
        build(
            "browser_open",
            json!({ "target": "/tmp/a.html", "placement": "beside", "focus": true })
        )
        .unwrap(),
        ControlCommand::Browser(BrowserCommand::Open {
            target: "/tmp/a.html".into(),
            workspace: None,
            focus: true,
            placement: Placement::Beside,
        })
    );
    assert_eq!(
        build("code_open", json!({ "path": "src/lib.rs", "line": 12 })).unwrap(),
        ControlCommand::Code(CodeCommand::Open(FileAt {
            path: "src/lib.rs".into(),
            line: Some(12),
        }))
    );
    assert_eq!(
        build("fan_out", json!({ "prompt": "Fix it", "agents": [{ "kind": "Claude", "count": 2 }, { "kind": "codex" }] })).unwrap(),
        ControlCommand::FanOut(FanOut {
            prompt: "Fix it".into(),
            agents: vec![("claude".into(), 2), ("codex".into(), 1)],
        })
    );
    assert_eq!(
        build(
            "wait",
            json!({ "for": "tab_closed", "tab_id": 4, "timeout_seconds": 5 })
        )
        .unwrap(),
        ControlCommand::Wait {
            what: WaitFor::TabClosed(4),
            timeout: 5,
        }
    );
    assert_eq!(
        build("memory_pin", json!({ "id": 3, "pinned": false })).unwrap(),
        ControlCommand::Memory(MemoryCommand::Pin {
            id: 3,
            pinned: false
        })
    );
    assert_eq!(
        build("memory_pin", json!({ "id": 3 })).unwrap(),
        ControlCommand::Memory(MemoryCommand::Pin {
            id: 3,
            pinned: true
        })
    );
}

#[test]
fn values_outside_a_tools_schema_are_refused() {
    assert!(matches!(
        build("memory_add", json!({ "text": "x", "kind": "secret" })),
        Err(Error::Choice { name: "kind", .. })
    ));
    assert!(matches!(
        build(
            "memory_add",
            json!({ "text": "x".repeat(sheprd_memory::MAX_TEXT + 1) })
        ),
        Err(Error::TooLong { name: "text", .. })
    ));
    assert!(matches!(
        build("wait", json!({ "for": "tab_closed" })),
        Err(Error::Missing { name: "tab_id" })
    ));
    assert!(matches!(
        build("ports_open", json!({ "port": 70_000 })),
        Err(Error::Range { name: "port", .. })
    ));
    assert!(matches!(
        build(
            "fan_out",
            json!({ "prompt": "x", "agents": [{ "kind": "claude", "count": 9 }] })
        ),
        Err(Error::Range { name: "count", .. })
    ));
    assert!(matches!(
        build("status", json!({ "shell": "rm -rf /" })),
        Err(Error::Unknown { .. })
    ));
}
