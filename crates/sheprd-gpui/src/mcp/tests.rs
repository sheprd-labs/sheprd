#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::{Map, Value, json};
use std::{cell::RefCell, rc::Rc};

mod arguments;
mod config_snippets;
mod handler;
mod serve;
mod tool_table;

/// What a fake `run` was asked: the command and the window.
type Ran = Rc<RefCell<Vec<(ControlCommand, Option<u64>)>>>;

/// A handler whose switch reads `on` and whose commands are recorded and
/// answered with `answer`, never reaching a socket.
fn sheprd(on: bool, answer: impl Fn() -> crate::Result<Output> + 'static) -> (impl Handler, Ran) {
    let ran = Ran::default();
    let record = Rc::clone(&ran);
    let handler = Sheprd {
        enabled: move || on,
        run: move |command: ControlCommand, context: &Context| {
            record.borrow_mut().push((command, context.window));
            answer()
        },
    };
    (handler, ran)
}

fn call(name: &str, arguments: Value) -> Call {
    let Value::Object(arguments) = arguments else {
        panic!("arguments must be an object");
    };
    Call {
        name: name.into(),
        arguments,
        client: Some(sheprd_mcp::ClientInfo::new("Test Client")),
    }
}

/// Builds `name`'s command from `arguments`, as a call would.
fn build(name: &str, arguments: Value) -> Result<ControlCommand, Error> {
    let spec = tools::find(name).expect("a tool by that name");
    let Value::Object(arguments) = arguments else {
        panic!("arguments must be an object");
    };
    let mut arguments = Arguments::new(arguments);
    let command = (spec.build)(&mut arguments)?;
    arguments.finish()?;
    Ok(command)
}

fn text(result: &ToolResult) -> &str {
    match &result.content[..] {
        [sheprd_mcp::Content::Text { text }] => text,
        other => panic!("expected one text, got {other:?}"),
    }
}

fn structured(result: &ToolResult) -> Value {
    Value::Object(result.structured_content.clone().unwrap_or_default())
}

fn done() -> crate::Result<Output> {
    Ok(Output::message("done"))
}
