//! What only the worker reads: the desktop's document, the core's shared
//! workspaces, and the saved tabs, kept between polls so an unchanged one
//! is not parsed or read again.

use super::{Result, kept::Kept, projection, spaces};
use crate::hub::Layout;
use sheprd_core_client::{
    Error,
    wire::{Doc, Terminal},
};
use sheprd_session::{SavedTabs, spaces::Tab};
use std::{path::Path, time::SystemTime};

/// The core's shared workspaces and tabs (`sheprd_session::spaces`).
pub(super) struct Shared {
    pub(super) spaces: sheprd_session::spaces::Spaces,
    /// Its tabs, with each recovered chat's terminal replaced by the one
    /// that took its place, as the last poll saw them.
    tabs: Vec<Tab>,
}

#[derive(Default)]
pub(super) struct Poller {
    sessions: Option<std::sync::Arc<crate::hub::sessions::Sessions>>,
    pub(super) named_scopes: std::collections::BTreeMap<String, String>,
    /// The core `doc` came from: versions count per core, so a wiped core
    /// can reach the old number with another document.
    boot: String,
    /// The document's version and its spaces, `None` when it has none.
    doc: Option<(u64, Option<spaces::Spaces>)>,
    checkouts: super::checkouts::Checkouts,
    names: spaces::Names,
    pub(super) recovery: sheprd_core_client::chat_recovery::Sweep,
    /// The recovery aliases the last poll saw: a phone names the replacement,
    /// while the saved file may still name the terminal it replaced.
    pub(super) replacements:
        std::collections::BTreeMap<sheprd_backend::TerminalId, sheprd_backend::TerminalId>,
    saved: Option<(SystemTime, Option<SavedTabs>)>,
    /// The shared document's version and what it holds, `None` when it was
    /// never written (version 0) or does not read: Sheprd's saved tabs then
    /// stand in for it.
    shared: Option<(u64, Option<Shared>)>,
    /// When each terminal agent last changed status, which herdr-core does
    /// not say: read marks know a reply by it (`control::read`).
    pub(super) times: crate::hub::recency::AgentTimes,
}

impl Poller {
    pub(super) fn following(sessions: std::sync::Arc<crate::hub::sessions::Sessions>) -> Self {
        Self {
            sessions: Some(sessions),
            ..Self::default()
        }
    }

    /// Takes the answer to reading the desktop's document. Only a core
    /// that says there is none clears the spaces: a slow answer says
    /// nothing about the document, and dropping them would make every
    /// space vanish from phones until the next poll. Another core's spaces
    /// are never kept: their addresses mean nothing here.
    pub(super) fn read_spaces(
        &mut self,
        boot: &str,
        answer: sheprd_core_client::Result<Doc>,
    ) -> Result<()> {
        if self.boot != boot {
            boot.clone_into(&mut self.boot);
            self.doc = None;
            self.shared = None;
        }
        match answer {
            Ok(doc) => {
                if self.doc.as_ref().is_none_or(|(at, _)| *at != doc.version) {
                    self.doc = Some((doc.version, spaces::Spaces::read(doc.value)));
                }
            }
            Err(Error::Refused { .. }) => self.doc = None,
            Err(Error::Lost) => return Err(Error::Lost.into()),
            Err(error) => tracing::debug!(%error, "kept the desktop's spaces"),
        }
        Ok(())
    }

    /// Takes the answer to reading the core's shared workspaces, after
    /// [`Self::read_spaces`] for the same core. As there, only a refusal
    /// forgets them, and a slow answer keeps what was read.
    pub(super) fn read_shared(&mut self, answer: sheprd_core_client::Result<Doc>) -> Result<()> {
        match answer {
            Ok(doc) if doc.version == 0 => self.shared = None,
            Ok(doc) => {
                if self
                    .shared
                    .as_ref()
                    .is_none_or(|(at, _)| *at != doc.version)
                {
                    let read = sheprd_session::spaces::Spaces::from_value(doc.value)
                        .inspect_err(|error| {
                            tracing::warn!(%error, "Read Sheprd's saved tabs for want of its shared workspaces");
                        })
                        .ok()
                        .map(|spaces| Shared {
                            tabs: spaces.tabs().to_vec(),
                            spaces,
                        });
                    self.shared = Some((doc.version, read));
                }
            }
            Err(Error::Refused { .. }) => self.shared = None,
            Err(Error::Lost) => return Err(Error::Lost.into()),
            Err(error) => tracing::debug!(%error, "kept the shared workspaces"),
        }
        Ok(())
    }

    /// The shared workspaces, while they stand in for Sheprd's saved tabs.
    pub(super) fn shared(&self) -> Option<&Shared> {
        self.shared.as_ref().and_then(|(_, shared)| shared.as_ref())
    }

    /// What Sheprd keeps for this core: the shared document once written,
    /// else `scope`'s saved tabs.
    pub(super) fn kept<'a>(&'a self, scope: Option<&'a str>) -> Option<Kept<'a>> {
        kept(
            self.shared(),
            self.saved.as_ref(),
            scope,
            &self.named_scopes,
        )
    }

    pub(super) fn project(
        &mut self,
        boot: &str,
        terminals: &[Terminal],
        tabs: Option<&Path>,
        scope: Option<&str>,
    ) -> Layout {
        if let Some(sessions) = &self.sessions {
            self.named_scopes = sessions.local_scopes();
        }
        if let Some(tabs) = tabs {
            self.refresh_saved(tabs);
        }
        let replacements = sheprd_core_client::chat_recovery::mapping(
            terminals
                .iter()
                .map(|terminal| (terminal.address.as_str(), &terminal.labels)),
        );
        if !replacements.is_empty()
            && let Some((_, Some(saved))) = &mut self.saved
        {
            for scope in scope
                .into_iter()
                .chain(self.named_scopes.values().map(String::as_str))
            {
                saved.remap(scope, &replacements);
            }
        }
        if let Some((_, Some(spaces))) = &mut self.doc {
            spaces.remap(&replacements);
        }
        if let Some((_, Some(shared))) = &mut self.shared {
            shared.tabs = shared.spaces.tabs().to_vec();
            for tab in &mut shared.tabs {
                super::kept::remap(tab, &replacements);
            }
        }
        let terminals: Vec<_> = terminals
            .iter()
            .filter(|terminal| {
                terminal
                    .labels
                    .get(sheprd_core_client::chat_recovery::PENDING_LABEL)
                    .is_none_or(String::is_empty)
            })
            .filter(|terminal| {
                !replacements
                    .keys()
                    .any(|old| old.as_str() == terminal.address)
            })
            .cloned()
            .collect();
        self.replacements = replacements;
        let terminals = terminals.as_slice();
        let shared = self.shared.as_ref().and_then(|(_, shared)| shared.as_ref());
        let kept = kept(shared, self.saved.as_ref(), scope, &self.named_scopes);
        let mut layout = match &self.doc {
            Some((_, Some(spaces))) => {
                let mut layout = spaces.project(boot, terminals, kept, &mut self.names);
                projection::lead_kept(&mut layout, kept);
                layout
            }
            _ => projection::project_kept(boot, terminals, kept),
        };
        self.checkouts.fill(&mut layout, std::time::Instant::now());
        layout
    }

    /// Whether Sheprd keeps workspace `id` for this core.
    pub(super) fn keeps(&self, scope: Option<&str>, id: &str) -> bool {
        self.kept(scope).is_some_and(|kept| kept.keeps(id))
    }

    /// Every terminal a tab Sheprd keeps for this core holds.
    pub(super) fn held(&self, scope: Option<&str>) -> std::collections::HashSet<String> {
        self.kept(scope).map(Kept::held).unwrap_or_default()
    }

    /// `scope`'s saved tab holding the terminal at `address`, as Sheprd
    /// knows a tab of the shared document it shows.
    pub(super) fn saved_tab_holding(&self, scope: &str, address: &str) -> Option<u64> {
        let terminal = sheprd_backend::TerminalId::new(address).ok()?;
        self.saved
            .as_ref()
            .and_then(|(_, saved)| saved.as_ref())?
            .core_tabs(scope)
            .find(|tab| tab.panes.contains(&terminal))
            .map(|tab| tab.id)
    }

    /// Whether the file owns this pane beside the current shared document.
    pub(super) fn local_tab_holds(&self, scope: Option<&str>, id: &str, address: &str) -> bool {
        let Some(kept) = self.kept(scope) else {
            return false;
        };
        let target = format!("{}{id}", super::saved::LOCAL_TAB);
        kept.tabs().into_iter().any(|tab| {
            tab.target == target
                && tab
                    .panes
                    .terminals()
                    .into_iter()
                    .any(|pane| pane.as_str() == address)
        })
    }

    /// The workspace of `scope`'s saved tab `tab`.
    pub(super) fn saved_workspace(&self, scope: &str, tab: u64) -> Option<String> {
        self.saved
            .as_ref()
            .and_then(|(_, saved)| saved.as_ref())?
            .core_tabs(scope)
            .find(|saved| saved.id == tab)
            .map(|saved| saved.workspace_id.to_owned())
    }

    fn refresh_saved(&mut self, path: &Path) {
        let changed = std::fs::metadata(path).and_then(|file| file.modified());
        match (changed, &self.saved) {
            (Ok(at), Some((read, _))) if at == *read => {}
            (Ok(at), _) => self.saved = Some((at, projection::read(path))),
            (Err(_), _) => self.saved = None,
        }
    }
}

fn kept<'a>(
    shared: Option<&'a Shared>,
    saved: Option<&'a (SystemTime, Option<SavedTabs>)>,
    scope: Option<&'a str>,
    named: &'a std::collections::BTreeMap<String, String>,
) -> Option<Kept<'a>> {
    let saved = saved.and_then(|(_, saved)| saved.as_ref());
    let doc = shared.map(|s| (&s.spaces, s.tabs.as_slice()));
    (doc.is_some() || saved.is_some()).then_some(Kept {
        doc,
        file: saved,
        primary: scope,
        named,
    })
}
