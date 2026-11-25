//! What the person typed in a chat's composer and has not sent, kept across
//! a restart of the app as the phone keeps its own, and marked with a pencil
//! on the agent's row so a draft is not forgotten. A draft belongs to the
//! chat it was typed in: it comes back only into the same program's
//! composer, and a different chat later shown in that pane drops it. Drafts
//! stay on this computer; every window shares them.

use crate::{SheprdWindow, state_file};
use gpui::{App, Context, Global, Subscription};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

/// Drafts kept; the least recently edited go first beyond this.
const MAX_DRAFTS: usize = 64;
/// Bytes of one draft's text; a longer one keeps its beginning.
const MAX_TEXT_BYTES: usize = 32 * 1024;
/// Bytes of a host, pane, or chat id. A longer one keeps no draft.
const MAX_FIELD_BYTES: usize = 1024;
/// Room for the most drafts kept, each as large as accepted: JSON escapes a
/// control character in six bytes, plus field names and punctuation.
const MAX_FILE_BYTES: u64 =
    (MAX_DRAFTS * (6 * (MAX_TEXT_BYTES + 3 * MAX_FIELD_BYTES) + 128) + 64) as u64;
const FILE: &str = "chat-drafts.json";
/// How long typing must pause before the drafts are written.
const SAVE_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

/// The pane a draft was typed in, named so the sidebar can find its row for
/// any listed host, not only the one the window shows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Place {
    /// A Herdr 1 pane, on the host the app lists by `endpoint`.
    Herdr { endpoint: String, pane: String },
    /// A herdr-core terminal, by the host-scoped key the window gives its
    /// chat (`core_pane::key_on`).
    Core { key: String },
}

impl Place {
    fn valid(&self) -> bool {
        let fields: &[&str] = match self {
            Self::Herdr { endpoint, pane } => &[endpoint, pane],
            Self::Core { key } => &[key],
        };
        fields
            .iter()
            .all(|field| !field.is_empty() && field.len() <= MAX_FIELD_BYTES)
    }
}

/// The places holding a draft, which the sidebar marks.
pub(crate) type Marks = BTreeSet<Place>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Draft {
    place: Place,
    /// The chat's endpoint id, so another chat in the pane does not get it.
    chat: String,
    text: String,
    /// Which edit this is, so a composer that never saw it cannot clear it.
    #[serde(default)]
    version: u64,
}

impl Draft {
    fn valid(&self) -> bool {
        self.place.valid()
            && !self.chat.is_empty()
            && self.chat.len() <= MAX_FIELD_BYTES
            && self.text.len() <= MAX_TEXT_BYTES
            && !self.text.trim().is_empty()
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    drafts: Vec<Draft>,
}

fn parse(bytes: &[u8]) -> crate::Result<Vec<Draft>> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    let mut drafts: Vec<Draft> = saved.drafts.into_iter().filter(Draft::valid).collect();
    // One draft per place, the latest edit winning, within the bound.
    let mut seen = BTreeSet::new();
    drafts.reverse();
    drafts.retain(|draft| seen.insert(draft.place.clone()));
    drafts.truncate(MAX_DRAFTS);
    drafts.reverse();
    Ok(drafts)
}

/// `text` within [`MAX_TEXT_BYTES`], cut at a character boundary.
fn bounded(text: &str) -> &str {
    if text.len() <= MAX_TEXT_BYTES {
        return text;
    }
    let mut end = MAX_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// What a composer holds of the stored draft at its place: the version it
/// restored or last wrote, which only it may clear.
pub(crate) type Seen = Option<u64>;

/// The app's drafts, oldest edit first.
#[derive(Default)]
pub(crate) struct Drafts {
    drafts: Vec<Draft>,
    marks: Arc<Marks>,
    /// The last version given out.
    version: u64,
    /// Edits not yet handed to the writer.
    dirty: bool,
    /// Whether a save is already waiting out [`SAVE_DELAY`].
    save_pending: bool,
    writer: Option<state_file::Writer<Saved>>,
    quitting: bool,
}

impl Global for Drafts {}

impl Drafts {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI. A missing or damaged file keeps nothing.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let drafts: Vec<Draft> = path
            .as_deref()
            .map(|path| {
                state_file::read(path, MAX_FILE_BYTES)
                    .and_then(|bytes| bytes.as_deref().map_or(Ok(Vec::new()), parse))
            })
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot restore the chat drafts");
                None
            })
            .unwrap_or_default();
        let writer = path.and_then(|path| {
            state_file::Writer::start("chat-drafts", path)
                .inspect_err(|error| tracing::warn!(%error, "Cannot start the chat drafts worker"))
                .ok()
        });
        let marks = Arc::new(drafts.iter().map(|draft| draft.place.clone()).collect());
        let version = drafts.iter().map(|draft| draft.version).max().unwrap_or(0);
        Self {
            drafts,
            marks,
            version,
            writer,
            ..Self::default()
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let drafts = cx.global_mut::<Self>();
            // What was typed within the last moment is saved too.
            drafts.flush();
            drafts.quitting = true;
            let writer = drafts.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    fn at_place(&self, place: &Place) -> Option<&Draft> {
        self.drafts.iter().find(|draft| draft.place == *place)
    }

    /// The draft typed in `chat` at `place`, with its version.
    fn text(&self, place: &Place, chat: &str) -> Option<(&str, u64)> {
        self.at_place(place)
            .filter(|draft| draft.chat == chat)
            .map(|draft| (draft.text.as_str(), draft.version))
    }

    /// What [`Self::set`] would leave the composer holding, when it would
    /// change nothing.
    fn unchanged(&self, place: &Place, chat: &str, text: &str, seen: Seen) -> Option<Seen> {
        let text = bounded(text);
        match self.at_place(place) {
            Some(draft) if draft.chat == chat && draft.text == text => Some(Some(draft.version)),
            // Only a composer that saw the stored draft may clear it: another
            // window's newer text stays when this one sends older text.
            Some(draft) if draft.chat == chat && text.trim().is_empty() => {
                (seen != Some(draft.version)).then_some(None)
            }
            Some(_) => None,
            None => text.trim().is_empty().then_some(None),
        }
    }

    /// Keeps `text` as the draft of `chat` at `place`, replacing whatever
    /// that place held; blank text removes it, if `seen` is the version
    /// stored. Returns the version the composer now holds.
    fn set(&mut self, place: &Place, chat: &str, text: &str, seen: Seen) -> Seen {
        if !place.valid() || chat.is_empty() || chat.len() > MAX_FIELD_BYTES {
            return None;
        }
        if let Some(held) = self.unchanged(place, chat, text, seen) {
            return held;
        }
        self.drafts.retain(|draft| draft.place != *place);
        let text = bounded(text);
        let held = if text.trim().is_empty() {
            None
        } else {
            self.version += 1;
            self.drafts.push(Draft {
                place: place.clone(),
                chat: chat.to_owned(),
                text: text.to_owned(),
                version: self.version,
            });
            if self.drafts.len() > MAX_DRAFTS {
                let excess = self.drafts.len() - MAX_DRAFTS;
                self.drafts.drain(..excess);
            }
            Some(self.version)
        };
        // A new set only when a pencil comes or goes, not with each key.
        let marks: Marks = self
            .drafts
            .iter()
            .map(|draft| draft.place.clone())
            .collect();
        if marks != *self.marks {
            self.marks = Arc::new(marks);
        }
        self.dirty = true;
        held
    }

    /// Hands the drafts to the writer, if anything changed since.
    fn flush(&mut self) {
        if !std::mem::take(&mut self.dirty) || self.quitting {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.save(Saved {
                drafts: self.drafts.clone(),
            });
        }
    }

    /// Saves a moment after typing stops rather than with each key, so the
    /// drafts are copied and written once a pause, not once a keystroke.
    fn save_soon(cx: &mut App) {
        let drafts = cx.global_mut::<Self>();
        if drafts.save_pending || drafts.writer.is_none() {
            return;
        }
        drafts.save_pending = true;
        cx.spawn(async move |cx: &mut gpui::AsyncApp| {
            cx.background_executor().timer(SAVE_DELAY).await;
            cx.update(|cx| {
                let drafts = cx.global_mut::<Self>();
                drafts.save_pending = false;
                drafts.flush();
            });
        })
        .detach();
    }

    /// The places holding a draft, shared until one comes or goes.
    pub(crate) fn current(cx: &App) -> Arc<Marks> {
        cx.try_global::<Self>()
            .map(|drafts| drafts.marks.clone())
            .unwrap_or_default()
    }

    /// The draft to show in a new composer for `chat` at `place`, with its
    /// version. A draft another chat left in that place is dropped, so its
    /// pencil goes.
    fn restore(cx: &mut App, place: &Place, chat: &str) -> Option<(String, u64)> {
        let drafts = cx.try_global::<Self>()?;
        if let Some((text, version)) = drafts.text(place, chat) {
            return Some((text.to_owned(), version));
        }
        Self::record(cx, place, chat, "", None);
        None
    }

    /// [`Self::set`], touching the global only when something changes, so
    /// windows are not told of every key that leaves the draft as it was.
    /// Creates an unsaved store for fixtures that never installed one.
    pub(super) fn record(cx: &mut App, place: &Place, chat: &str, text: &str, seen: Seen) -> Seen {
        if let Some(held) = cx
            .try_global::<Self>()
            .and_then(|drafts| drafts.unchanged(place, chat, text, seen))
        {
            return held;
        }
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        let held = cx.global_mut::<Self>().set(place, chat, text, seen);
        Self::save_soon(cx);
        held
    }

    #[cfg(test)]
    fn finish(&mut self) {
        self.flush();
        if let Some(writer) = self.writer.take() {
            writer.finish();
        }
    }
}

impl SheprdWindow {
    /// Follows the drafts every window keeps, redrawing the sidebar only
    /// when a pencil comes or goes.
    pub(crate) fn observe_chat_drafts(cx: &mut Context<Self>) -> Subscription {
        cx.observe_global::<Drafts>(|this, cx| {
            let marks = Drafts::current(cx);
            if !Arc::ptr_eq(&marks, &this.chat_drafts) {
                this.chat_drafts = marks;
                cx.notify();
            }
        })
    }

    /// Where the chat for `target` keeps its draft: its Herdr 1 pane on the
    /// host this window shows, or its herdr-core terminal.
    fn chat_draft_place(&self, target: &super::Target) -> Option<Place> {
        match target.surface {
            super::Surface::Herdr => Some(Place::Herdr {
                endpoint: self.endpoints.get(self.selected_endpoint)?.id.clone(),
                pane: target.pane_id.clone(),
            }),
            super::Surface::Core(_) | super::Surface::RemoteCore { .. } => Some(Place::Core {
                key: target.pane_id.clone(),
            }),
        }
    }

    /// The draft a new composer for the chat in `pane_id` starts with, and
    /// its version.
    pub(super) fn restored_chat_draft(&self, pane_id: &str, cx: &mut App) -> Option<(String, u64)> {
        let target = &self.chats.pane(pane_id)?.target;
        let place = self.chat_draft_place(target)?;
        Drafts::restore(cx, &place, target.endpoint.as_str())
    }

    /// Keeps what the composer of the chat in `pane_id` now holds.
    pub(super) fn save_chat_draft(&mut self, pane_id: &str, cx: &mut App) {
        let Some(pane) = self.chats.pane(pane_id) else {
            return;
        };
        let Some(composer) = &pane.composer else {
            return;
        };
        let Some(place) = self.chat_draft_place(&pane.target) else {
            return;
        };
        let text = composer.read(cx).text().to_owned();
        let chat = pane.target.endpoint.as_str().to_owned();
        let seen = Drafts::record(cx, &place, &chat, &text, pane.draft_seen);
        if let Some(pane) = self.chats.pane_mut(pane_id) {
            pane.draft_seen = seen;
        }
    }
}

#[cfg(test)]
mod tests;
