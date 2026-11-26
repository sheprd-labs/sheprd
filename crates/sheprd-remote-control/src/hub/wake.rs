//! A phone on a machine without Herdr 1, such as a cloud machine set up
//! with only the pinned core, starts that core rather than finding nothing
//! running: when it opens its home view, and when it asks for new work.
//! Where Herdr 1 is installed, moving to Herdr 2 is the user's choice: a
//! core starts there for new work only once they chose "New terminals use
//! Herdr 2" (`herdr_2_terminals` in that computer's Sheprd settings), even
//! while Herdr 1 runs. Herdr 1 keeps running either way.

use super::{Hub, Result, core, locate, locate_on, on_core};
use std::{
    ffi::OsStr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// Herdr 1's program, whose presence leaves the core unstarted.
const HERDR: &str = if cfg!(windows) { "herdr.exe" } else { "herdr" };
/// How long after a failed start a phone's view tries again; new work
/// always tries, and says why it failed.
const RETRY: Duration = Duration::from_secs(60);

/// What asks for this machine's core to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wake {
    /// A phone opening its home view.
    View,
    /// A phone's new tab, chat, or workspace.
    Work,
}

impl Hub {
    /// The core new work goes to: the one answering, else this machine's,
    /// started now when Herdr 1 neither answers nor is installed here.
    /// `Ok(None)` when the work is Herdr 1's; an error when a start failed.
    pub(super) async fn core_for_work(&self) -> Result<Option<Arc<core::Source>>> {
        self.start_core(Wake::Work).await
    }

    /// [`Self::core_for_work`] for `wake`: a view does not start a core again
    /// within a minute of a failed start; new work always may.
    async fn start_core(&self, wake: Wake) -> Result<Option<Arc<core::Source>>> {
        if let Some(core) = self.active_core() {
            return Ok(Some(core));
        }
        let Some(core) = self.startable() else {
            return Ok(None);
        };
        let answering =
            self.primary.borrow().connected || self.sessions.first_connected().is_some();
        // The login shell's PATH can take a moment to read the first time.
        let Some(binary) = on_core(move || Ok(core_program(answering))).await? else {
            return Ok(None);
        };
        let mut failed = self.waking.lock().await;
        // Another phone's request may have started it meanwhile.
        if core.connected() {
            return Ok(Some(core));
        }
        // Checked under the lock, so views queued behind a failed start do
        // not each try again.
        if wake == Wake::View && failed.is_some_and(|at| at.elapsed() < RETRY) {
            return Ok(None);
        }
        tracing::info!(binary = %binary.display(), "Starting herdr-core for a phone");
        let started = core.clone();
        match on_core(move || started.wake(&binary)).await {
            Ok(()) => {
                *failed = None;
                Ok(Some(core))
            }
            Err(error) => {
                tracing::warn!(%error, "Could not start herdr-core for a phone");
                *failed = Some(Instant::now());
                Err(error)
            }
        }
    }

    /// Starts this machine's core in the background for a phone opening its
    /// home view with nothing answering, so the phone finds a workspace to
    /// start work in; at most once a minute after a failure.
    pub(crate) fn wake_for_phone(self: &Arc<Self>) {
        if self.layout().connected || self.startable().is_none() {
            return;
        }
        // A start underway, or a recent failure: nothing to queue.
        let idle = self
            .waking
            .try_lock()
            .is_ok_and(|failed| failed.is_none_or(|at| at.elapsed() >= RETRY));
        if !idle {
            return;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            let _ = hub.start_core(Wake::View).await;
        });
    }

    /// This machine's core source; whether Herdr 1 leaves it unstarted is
    /// [`core_program`]'s to say.
    fn startable(&self) -> Option<Arc<core::Source>> {
        // Tests' hubs look for no programs, and start nothing.
        self.core.clone().filter(|_| self.search_program)
    }
}

/// The program to start this machine's core with: when Herdr 1 neither
/// `answering` nor installed here, or the user chose Herdr 2 for new
/// terminals. Blocking: it reads settings and the login shell's PATH.
fn core_program(answering: bool) -> Option<PathBuf> {
    let selected = settings_path().map(|path| chosen(&path));
    if selected == Some(sheprd_backend::TerminalBackend::Tmux) {
        return None;
    }
    let chosen = selected == Some(sheprd_backend::TerminalBackend::Core);
    if answering && !chosen {
        return None;
    }
    let login = sheprd_client::login_env::path();
    if !chosen && find(HERDR, login.as_deref()).is_some() {
        return None;
    }
    find(sheprd_core_client::PROGRAM, login.as_deref())
}

/// Lines of scrollback kept when the user's config names none.
#[cfg(feature = "tmux")]
pub(super) const DEFAULT_SCROLLBACK: usize = 10_000;
#[cfg(feature = "tmux")]
const MIN_SCROLLBACK: usize = 1_000;
#[cfg(feature = "tmux")]
const MAX_SCROLLBACK: usize = 100_000;

/// Sheprd's settings on this computer.
pub(super) fn settings_path() -> Option<PathBuf> {
    let root = sheprd_client::local_path::config_root().ok()?;
    Some(root.join("herdr/config-gpui.toml"))
}

fn read_settings(path: &std::path::Path) -> (Option<toml::Table>, Option<toml::Table>) {
    fn read(path: &std::path::Path) -> Option<toml::Table> {
        sheprd_client::settings_files::read(path)
            .ok()??
            .parse()
            .ok()
    }
    (
        read(&sheprd_client::settings_files::overrides_path(path)),
        read(path),
    )
}

/// Whether the settings at `path` say new terminals use Herdr 2: the
/// user's overrides first, then the file itself, off when neither says.
pub(super) fn chosen(path: &std::path::Path) -> sheprd_backend::TerminalBackend {
    use sheprd_backend::TerminalBackend;
    let (overrides, settings) = read_settings(path);
    let value = |key| {
        overrides
            .as_ref()
            .and_then(|table| table.get(key))
            .or_else(|| settings.as_ref().and_then(|table| table.get(key)))
    };
    let backend = value("terminal_backend").and_then(|value| value.clone().try_into().ok());
    let legacy = value("herdr_2_terminals")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    TerminalBackend::selected(backend, legacy, cfg!(feature = "tmux"))
}

/// Configured scrollback lines from `path` (overrides first, then settings),
/// clamped to the desktop's bounds or defaulting to [`DEFAULT_SCROLLBACK`].
#[cfg(feature = "tmux")]
pub(super) fn scrollback(path: &std::path::Path) -> usize {
    let (overrides, settings) = read_settings(path);
    let value = overrides
        .as_ref()
        .and_then(|table| table.get("herdr_2_scrollback"))
        .or_else(|| {
            settings
                .as_ref()
                .and_then(|table| table.get("herdr_2_scrollback"))
        });
    value
        .and_then(toml::Value::as_integer)
        .and_then(|lines| usize::try_from(lines).ok())
        .map_or(DEFAULT_SCROLLBACK, |lines| {
            lines.clamp(MIN_SCROLLBACK, MAX_SCROLLBACK)
        })
}

/// `program` where the daemon looks, else on the login shell's `login`
/// PATH: a service's own PATH can leave out a directory the user's shell
/// adds, such as Homebrew's, and a program there is still installed.
pub(super) fn find(program: &str, login: Option<&OsStr>) -> Option<PathBuf> {
    locate(program).or_else(|| login.and_then(|path| locate_on(program, Some(path))))
}
