//! What Add Device offers when a host cannot be saved as it is, and how it
//! asks: install herdr-core, continue setup, restart an existing Herdr 1
//! server, or install the user's SSH key.

use crate::i18n::fl;
use sheprd_client::{HostProbe, SshFailure};

/// Why setup needs the user's input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::menu::devices) enum Offer {
    /// Neither Herdr 1 nor herdr-core was found: install pinned herdr-core.
    Install,
    /// Only an outdated Herdr 1 was found: install pinned herdr-core instead.
    Update,
    /// SSH needs a prompt, or saving without a terminal failed.
    Terminal,
    /// Herdr restarts the host's server before saving, once confirmed.
    Restart,
    /// SSH accepted none of the user's keys: install one with the password,
    /// then check the host again.
    InstallKey,
}

impl Offer {
    /// `None` when a usable daemon is present, so the device is saved directly.
    pub(in crate::menu::devices) fn for_probe(probe: HostProbe) -> Option<Self> {
        match probe {
            HostProbe::Tmux | HostProbe::Core | HostProbe::Running | HostProbe::Stopped => None,
            HostProbe::Missing => Some(Self::Install),
            HostProbe::Outdated => Some(Self::Update),
            HostProbe::SshFailed(SshFailure::Auth) => Some(Self::InstallKey),
            HostProbe::SshFailed(_) => Some(Self::Terminal),
        }
    }

    /// Whether accepting this offer installs pinned herdr-core directly over
    /// SSH without a local terminal or Herdr 1's CLI.
    pub(super) fn installs_core(self) -> bool {
        matches!(self, Self::Install | Self::Update)
    }

    /// `user` is the login `ssh` resolved for the target, when known: a target
    /// without one logs in as the local user, which may not be the account
    /// the user's keys are installed for.
    pub(in crate::menu::devices) fn question(self, target: &str, user: Option<&str>) -> String {
        match self {
            Self::Install => fl!("add-device-ask-install", target = target),
            Self::Update => fl!("add-device-ask-update", target = target),
            _ if cfg!(windows) => self.terminal_instructions(target, user),
            Self::Terminal => fl!("add-device-ask-terminal", target = target),
            Self::Restart => fl!("add-device-ask-restart", target = target),
            Self::InstallKey => match implied_user(target, user) {
                Some(user) => fl!(
                    "add-device-ask-key-user",
                    target = target,
                    user = user,
                    example = example_login(target),
                ),
                None => fl!("add-device-ask-key", target = target),
            },
        }
    }

    pub(super) fn action(self) -> String {
        match self {
            Self::Install | Self::Update => fl!("add-device-install"),
            Self::Terminal | Self::Restart => fl!("add-device-open-terminal"),
            Self::InstallKey => fl!("add-device-set-up-key"),
        }
    }

    /// Windows cannot host interactive setup yet, so it names the CLI command.
    fn terminal_instructions(self, target: &str, user: Option<&str>) -> String {
        match self {
            Self::Install => fl!("add-device-ask-install", target = target),
            Self::Update => fl!("add-device-ask-update", target = target),
            Self::Terminal => fl!("add-device-terminal-input", target = target),
            Self::Restart => fl!("add-device-terminal-restart", target = target),
            Self::InstallKey => match implied_user(target, user) {
                Some(user) => fl!(
                    "add-device-terminal-key-user",
                    target = target,
                    user = user,
                    example = example_login(target),
                ),
                None => fl!("add-device-terminal-key", target = target),
            },
        }
    }
}

/// `target` with a placeholder login, to show how to name another account.
fn example_login(target: &str) -> String {
    with_user(target, &fl!("add-device-example-user"))
}

/// The resolved login, only when `target` does not name one itself: the
/// dialog then has not shown which account SSH tried.
fn implied_user<'a>(target: &str, user: Option<&'a str>) -> Option<&'a str> {
    let named = target
        .strip_prefix("ssh://")
        .unwrap_or(target)
        .contains('@');
    user.filter(|_| !named)
}

/// `target` logging in as `user`; an `ssh://` URI takes the user inside it.
fn with_user(target: &str, user: &str) -> String {
    match target.strip_prefix("ssh://") {
        Some(rest) => format!("ssh://{user}@{rest}"),
        None => format!("{user}@{target}"),
    }
}
