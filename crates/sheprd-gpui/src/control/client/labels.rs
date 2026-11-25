//! Labels belong to the host that serves the agent list.
use super::Output;
use sheprd_remote_control::control::labels::{Command, Error};

pub(super) fn run(command: Command) -> crate::Result<Output> {
    let tags = command
        .run(&crate::remote::state_dir()?)
        .map_err(|error| match error {
            Error::Control(error) => crate::Error::from(crate::remote::Error::from(error)),
            Error::Unexpected => crate::remote::Error::Unexpected.into(),
            Error::Socket(error) => error.into(),
            Error::Core(error) => crate::processes::Error::Core(error).into(),
            Error::NoTarget | Error::SocketPath => crate::control::Error::Rejected {
                code: crate::control::ErrorCode::InvalidRequest,
                message: error.to_string(),
            }
            .into(),
        })?;
    Output::data(tags.join("\n"), &serde_json::json!({"tags": tags}))
}
