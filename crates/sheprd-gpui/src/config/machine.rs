//! `[machine]`: virtual Macs shared between the user's computers
//! (`docs/remote-machines-plan.md`).
use serde::Deserialize;

/// The longest host list `borrow_from` keeps.
const MAX_BORROW_FROM: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MachineConfig {
    /// Whether the user's other computers may borrow this Mac's virtual
    /// Macs over SSH. A Mac whose slots Tart or another program uses never
    /// lends, whatever this says.
    pub lend: bool,
    /// Hosts, by ID or label, the user chose to borrow from without being
    /// asked each time.
    pub borrow_from: Vec<String>,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            lend: true,
            borrow_from: Vec::new(),
        }
    }
}

impl MachineConfig {
    /// Whether the user chose to borrow from `host` without being asked.
    pub(crate) fn borrows_from(&self, host: &str) -> bool {
        self.borrow_from.iter().any(|known| known == host)
    }

    /// The list with `host` added, for saving; `None` when it is there
    /// already or the list is full.
    pub(crate) fn adding(&self, host: &str) -> Option<Vec<String>> {
        if self.borrows_from(host) || self.borrow_from.len() >= MAX_BORROW_FROM {
            return None;
        }
        let mut hosts = self.borrow_from.clone();
        hosts.push(host.to_owned());
        hosts.sort();
        Some(hosts)
    }
}

#[cfg(test)]
mod tests;
