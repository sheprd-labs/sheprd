//! A phone's new workspace on the core, kept in the core's shared document
//! (`sheprd_session::spaces`) by the daemon itself, so every client lists
//! it at once with no desktop running to adopt it. Its terminal keeps the
//! label asking a desktop to keep it (`NEW_WORKSPACE_LABEL`), for desktops
//! of earlier versions.

use super::{Error, Result, Source, lock};
use sheprd_backend::TerminalId;
use sheprd_core_client::spaces;
use sheprd_session::{
    panes::Panes,
    spaces::{Edit, ID_PREFIX, Tab, Workspace},
};
use std::{collections::BTreeMap, path::Path};

/// How a tab id the daemon hands out starts, as a desktop's do.
const TAB_PREFIX: &str = "tab-";

impl Source {
    /// Keeps a workspace in `cwd` in the shared document, unless one there
    /// already has that folder, with a tab titled `title` for the terminal
    /// of `pane`. Answers the workspace's id; `None` while the document was
    /// never written, as Sheprd then adopts the terminal from its label and
    /// writes the document from what it keeps.
    pub(in crate::hub) fn share_new_workspace(
        &self,
        cwd: &str,
        pane: &str,
        title: &str,
    ) -> Result<Option<String>> {
        if lock(&self.poller).shared().is_none() {
            return Ok(None);
        }
        let (core, terminal) = self.terminal(pane)?;
        let address = TerminalId::new(terminal.address.as_str())
            .map_err(|error| Error::Client(error.into()))?;
        let (space_id, tab_id) = (new_id(ID_PREFIX)?, new_id(TAB_PREFIX)?);
        let label = Path::new(cwd)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(cwd)
            .to_owned();
        let mut kept = None;
        spaces::change(&core, |spaces| {
            let joined = spaces
                .workspaces()
                .iter()
                .find(|workspace| workspace.cwd == cwd)
                .map(|workspace| workspace.id.clone());
            let id = match joined {
                Some(id) => id,
                None => {
                    spaces.apply(Edit::KeepWorkspace(Workspace {
                        id: space_id.clone(),
                        label: label.clone(),
                        custom: false,
                        cwd: cwd.to_owned(),
                    }))?;
                    space_id.clone()
                }
            };
            if spaces.tab_holding(&address).is_none() {
                spaces.apply(Edit::OpenTab(Tab {
                    id: tab_id.clone(),
                    workspace_id: id.clone(),
                    title: title.to_owned(),
                    name: None,
                    names: BTreeMap::new(),
                    panes: Panes::single(address.clone()),
                    zoomed: None,
                }))?;
            }
            kept = Some(id);
            Ok(())
        })?;
        Ok(kept)
    }
}

/// `prefix` and 16 random hex digits: ids no other client hands out.
fn new_id(prefix: &str) -> Result<String> {
    use std::fmt::Write as _;
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(Error::Random)?;
    let mut id = prefix.to_owned();
    for byte in bytes {
        let _ = write!(id, "{byte:02x}");
    }
    Ok(id)
}
