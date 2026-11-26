//! Command transport for a tmux server on another host.
use super::{Error, Result, Tmux};
use sheprd_client::shell_quote;
use std::{process::Command, sync::Arc};

/// Keep the host's channel or temporary credentials alive until the command
/// completes, or until a new control client acknowledges attachment.
pub trait CommandHost: Send + Sync {
    fn with_command(&self, script: &str, persistent: bool, run: CommandTask<'_>) -> Result<()>;
}

pub type CommandTask<'a> = Box<dyn FnOnce(Command) -> Result<()> + 'a>;

impl Tmux {
    pub fn on_host(mut self, host: impl CommandHost + 'static) -> Self {
        self.target = Some(Arc::new(host));
        self
    }

    pub(super) fn with_command<T>(
        &self,
        command: Command,
        persistent: bool,
        run: impl FnOnce(Command) -> Result<T>,
    ) -> Result<T> {
        let Some(host) = &self.target else {
            return run(command);
        };
        with_script(host.as_ref(), &remote_script(&command)?, persistent, run)
    }
}

pub(super) fn with_script<T>(
    host: &dyn CommandHost,
    script: &str,
    persistent: bool,
    run: impl FnOnce(Command) -> Result<T>,
) -> Result<T> {
    let mut output = None;
    host.with_command(
        script,
        persistent,
        Box::new(|command| {
            output = Some(run(command)?);
            Ok(())
        }),
    )?;
    output.ok_or(Error::Invalid)
}

pub(super) fn remote_script(cmd: &Command) -> Result<String> {
    // Noninteractive SSH shells commonly omit Homebrew's bin directory.
    let mut script = String::from(
        "unset TMUX\nPATH=\"$PATH:/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin\"\nexport PATH\nexec tmux",
    );
    for arg in cmd.get_args() {
        script.push(' ');
        script.push_str(&shell_quote(arg.to_str().ok_or(Error::Invalid)?));
    }
    Ok(script)
}
