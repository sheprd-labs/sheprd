/// The user's choice for new terminals. With no explicit
/// choice the older herdr_2_terminals setting still selects Herdr 1 or 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum TerminalBackend {
    #[serde(rename = "herdr")]
    Herdr,
    #[serde(rename = "herdr-core")]
    Core,
    #[serde(rename = "tmux")]
    Tmux,
}

impl TerminalBackend {
    pub fn available(self, tmux: bool) -> bool {
        self != Self::Tmux || tmux
    }

    /// An unavailable backend falls back to the pre-selector preference.
    pub fn selected(choice: Option<Self>, legacy_core: bool, tmux: bool) -> Self {
        choice
            .filter(|backend| backend.available(tmux))
            .unwrap_or(if legacy_core { Self::Core } else { Self::Herdr })
    }

    pub fn owner(id: &crate::TerminalId) -> Self {
        if id.is_tmux() { Self::Tmux } else { Self::Core }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Herdr => "herdr",
            Self::Core => "herdr-core",
            Self::Tmux => "tmux",
        }
    }
}
