use super::*;

fn arguments(value: Value) -> Arguments {
    let Value::Object(map) = value else {
        panic!("an object");
    };
    Arguments::new(map)
}

#[test]
fn absent_and_null_read_as_nothing() {
    let mut read = arguments(json!({ "a": null }));
    assert_eq!(read.text("a", 8).unwrap(), None);
    assert_eq!(read.number("b", 0..=1).unwrap(), None);
    assert!(!read.flag("c").unwrap());
    assert_eq!(read.optional_flag("d").unwrap(), None);
    assert!(read.agents("e", 8).unwrap().is_empty());
    read.finish().unwrap();
}

#[test]
fn wrong_types_are_refused_with_the_arguments_name() {
    let mut read = arguments(json!({ "a": 1, "b": "1", "c": "yes", "d": 1.5, "e": {} }));
    assert!(matches!(
        read.text("a", 8),
        Err(Error::Type { name: "a", .. })
    ));
    assert!(matches!(
        read.number("b", 0..=9),
        Err(Error::Type { name: "b", .. })
    ));
    assert!(matches!(read.flag("c"), Err(Error::Type { name: "c", .. })));
    assert!(matches!(
        read.number("d", 0..=9),
        Err(Error::Type { name: "d", .. })
    ));
    assert!(matches!(
        read.agents("e", 8),
        Err(Error::Type { name: "e", .. })
    ));
}

#[test]
fn bounds_hold_for_text_numbers_and_lists() {
    let mut read = arguments(json!({ "a": "toolong", "b": -1, "c": 10, "d": "  " }));
    assert!(matches!(
        read.text("a", 3),
        Err(Error::TooLong { name: "a", max: 3 })
    ));
    assert!(matches!(
        read.number("b", 0..=9),
        Err(Error::Range { name: "b", .. })
    ));
    assert!(matches!(
        read.number("c", 0..=9),
        Err(Error::Range { name: "c", .. })
    ));
    assert!(matches!(
        read.required_text("d", 8),
        Err(Error::Missing { name: "d" })
    ));
    let many: Vec<Value> = (0..17).map(|_| json!({ "kind": "claude" })).collect();
    let mut read = arguments(json!({ "agents": many }));
    assert!(matches!(read.agents("agents", 8), Err(Error::Range { .. })));
}

#[test]
fn an_agent_entry_takes_only_kind_and_count() {
    let mut read = arguments(json!({ "agents": [{ "kind": "claude", "model": "x" }] }));
    assert!(matches!(
        read.agents("agents", 8),
        Err(Error::Unknown { .. })
    ));
}

#[test]
fn an_unread_argument_is_refused_and_named_safely() {
    let read = arguments(json!({ "evil\u{1b}[2J": 1 }));
    let Err(Error::Unknown { name }) = read.finish() else {
        panic!("an unknown argument");
    };
    assert_eq!(name, "evil[2J");
}
