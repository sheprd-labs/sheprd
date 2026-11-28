//! Control client handler for `sheprd modal ...`.

use super::{Context, Output, unexpected};
use crate::{
    cli::control::ModalCommand,
    control::protocol::{Request, Response, modal::Reply},
};

pub(super) fn run(command: ModalCommand, context: &Context) -> crate::Result<Output> {
    let ModalCommand::Op(op) = command;
    match context.call(Request::Modal, op)? {
        Response::Modal {
            reply: Reply::Shown { id },
        } => Output::data(
            format!("Shown modal {id}"),
            &serde_json::json!({ "id": id }),
        ),
        Response::Modal {
            reply: Reply::Cleared { count },
        } => Output::data(
            format!("Cleared {count} modal(s)"),
            &serde_json::json!({ "cleared": count }),
        ),
        Response::Modal {
            reply: Reply::Listed { items },
        } => {
            let text = if items.is_empty() {
                "No active modals".to_owned()
            } else {
                items
                    .iter()
                    .map(|item| {
                        format!(
                            "{}\t{}\t{:?}\t{}",
                            item.id, item.kind, item.scope, item.title
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            Output::data(text, &items)
        }
        Response::Done { message } => Ok(Output::message(message)),
        other => Err(unexpected(other)),
    }
}
