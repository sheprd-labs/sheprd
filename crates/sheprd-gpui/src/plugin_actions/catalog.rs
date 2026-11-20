//! The plugin actions one host offers, and the rows a menu shows from them.
//! Pure: built from Herdr's `plugin.action.list` and `plugin.list` answers,
//! which are daemon data, and read by the menus while they render.

use serde::Deserialize;

/// Where a plugin declares an action belongs, as Herdr's manifests name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Context {
    Global,
    Workspace,
    Tab,
    Pane,
    Selection,
    /// A context this client does not know yet, which no menu shows.
    #[serde(other)]
    Other,
}

/// An operating system an action is limited to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Platform {
    Linux,
    Macos,
    Windows,
    #[serde(other)]
    Other,
}

impl Platform {
    /// The platform `uname -s` names, as Herdr's platform checks see it.
    pub(crate) fn of_uname(name: &str) -> Option<Self> {
        match name.trim() {
            "Darwin" => Some(Self::Macos),
            "Linux" => Some(Self::Linux),
            _ => None,
        }
    }
}

/// One entry of `plugin.action.list`. Herdr lists every installed plugin's
/// actions, disabled ones included, with the platforms each runs on.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Listed {
    pub(crate) plugin_id: String,
    pub(crate) action_id: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) contexts: Vec<Context>,
    #[serde(default)]
    pub(crate) platforms: Option<Vec<Platform>>,
}

/// One entry of `plugin.list`: what says whether a plugin is enabled, and
/// the name its menu heading shows.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Installed {
    pub(crate) plugin_id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
}

/// An action a menu may offer: its plugin is enabled, it runs on the host's
/// platform, and it names at least one context.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Action {
    plugin_id: String,
    plugin_name: String,
    action_id: String,
    title: String,
    description: Option<String>,
    contexts: Vec<Context>,
}

/// The actions one host offers, in menu order: by plugin name, then title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Catalog {
    actions: Vec<Action>,
}

/// What a menu is for, which picks the contexts it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Workspace,
    Tab,
    /// A pane, and whether text is selected in it, which adds the actions
    /// that take a selection.
    Pane {
        selection: bool,
    },
}

/// One menu row: an action and how it is invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) plugin_id: String,
    pub(crate) action_id: String,
    /// One line: control characters a manifest may hold become spaces.
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    /// The plugin's name when this row starts its plugin's group, which a
    /// heading above it shows.
    pub(crate) heading: Option<String>,
    /// Whether the action runs on the pane's selection.
    pub(crate) takes_selection: bool,
}

impl Catalog {
    /// The actions menus may offer. `platform` is the host's, when known; an
    /// unknown platform leaves the check to Herdr, which refuses the action.
    pub(crate) fn new(
        platform: Option<Platform>,
        listed: Vec<Listed>,
        installed: &[Installed],
    ) -> Self {
        let mut actions: Vec<Action> = listed
            .into_iter()
            .filter(|action| {
                platform.is_none_or(|platform| {
                    action
                        .platforms
                        .as_ref()
                        .is_none_or(|platforms| platforms.contains(&platform))
                })
            })
            .filter_map(|action| {
                // A plugin `plugin.list` does not name was removed between the
                // two answers, so it is left out like a disabled one.
                let plugin = installed
                    .iter()
                    .find(|plugin| plugin.plugin_id == action.plugin_id && plugin.enabled)?;
                Some(Action {
                    plugin_name: one_line(&plugin.name),
                    title: one_line(&action.title),
                    description: action.description.filter(|text| !text.trim().is_empty()),
                    plugin_id: action.plugin_id,
                    action_id: action.action_id,
                    contexts: action.contexts,
                })
            })
            .collect();
        actions.sort_by_cached_key(|action| {
            (
                action.plugin_name.to_lowercase(),
                action.plugin_id.clone(),
                action.title.to_lowercase(),
                action.action_id.clone(),
            )
        });
        Self { actions }
    }

    /// The rows a menu for `scope` shows, grouped by plugin.
    pub(crate) fn rows(&self, scope: Scope) -> Vec<Row> {
        let mut rows: Vec<Row> = Vec::new();
        for action in &self.actions {
            let has = |context| action.contexts.contains(&context);
            let (shown, takes_selection) = match scope {
                Scope::Workspace => (has(Context::Workspace), false),
                Scope::Tab => (has(Context::Tab), false),
                Scope::Pane { selection } => {
                    let takes_selection = selection && has(Context::Selection);
                    (has(Context::Pane) || takes_selection, takes_selection)
                }
            };
            if !shown {
                continue;
            }
            let starts_group = rows
                .last()
                .is_none_or(|last| last.plugin_id != action.plugin_id);
            rows.push(Row {
                plugin_id: action.plugin_id.clone(),
                action_id: action.action_id.clone(),
                title: action.title.clone(),
                description: action.description.clone(),
                heading: starts_group.then(|| action.plugin_name.clone()),
                takes_selection,
            });
        }
        rows
    }
}

/// `text` on one line, so a manifest's newline cannot break a menu row.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}
