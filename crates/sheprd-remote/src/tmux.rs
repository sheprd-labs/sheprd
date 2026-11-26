//! Run a tmux control client or a short tmux command on its owning device.
use sheprd_client::{ConnectTarget, SshLane};
use sheprd_tmux::{CommandHost, CommandTask, Error, Tmux};

struct Device(ConnectTarget);

/// A saved session may be restored before its device is available.
pub fn on_host(tmux: Tmux, target: &ConnectTarget) -> Option<Tmux> {
    target
        .is_remote()
        .then(|| tmux.on_host(Device(target.clone())))
}

impl CommandHost for Device {
    fn with_command(
        &self,
        script: &str,
        persistent: bool,
        run: CommandTask<'_>,
    ) -> Result<(), Error> {
        match &self.0 {
            ConnectTarget::Ssh { target, .. } if persistent => {
                run(sheprd_client::ssh::script_command(target, script)?)
            }
            ConnectTarget::Ssh { target, .. } => {
                let (command, _channel) =
                    sheprd_client::ssh::shared_script_command(target, script, SshLane::Quick)?;
                run(command)
            }
            ConnectTarget::Wsl { distro, .. } => {
                run(sheprd_client::wsl::script_command(distro, script)?)
            }
            #[cfg(feature = "cloud")]
            ConnectTarget::Cloud {
                provider,
                account,
                id,
                ..
            } => crate::cloud::with_command(*provider, account, id, &false.into(), |mut cmd| {
                cmd.arg(format!("/bin/sh -c {}", sheprd_client::shell_quote(script)));
                run(cmd)
            })?,
            _ => Err(Error::Invalid),
        }
    }
}
