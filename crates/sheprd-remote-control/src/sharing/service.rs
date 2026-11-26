//! Host-side coordinator connecting live `Hub` chats and trusted `Relays` to
//! `sharing::worker::Owner` and per-share `sharing::gateway::Route` listeners.
//!
//! No route starts on daemon boot; shares are created only after explicit owner
//! confirmation over the private local control socket.

use crate::{
    control::sharing::{
        PreviewEntry, PreviewRole, ShareCreated, ShareHistory, ShareLink, SharePreview,
        ShareSummary,
    },
    hub::{Chat, Event, Hub},
    relay::Relays,
    sharing::{
        Content, Error, GrantId, History, MAX_LIFETIME, Result, ShareId,
        gateway::Route,
        link,
        projection::Projection,
        worker::{Feed, Owner, Snapshot},
    },
    web::now_ms,
};
use secrecy::ExposeSecret;
use sheprd_chat_protocol::{EndpointId, EntryKind, ToolContent, Transcript, sharing::Update};
use sheprd_relay::{Endpoint, client};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{
    sync::{broadcast::error::RecvError, watch},
    task::JoinHandle,
};

const MAX_PREVIEW_ENTRIES: usize = 32;
const MAX_PREVIEW_CHARS: usize = 240;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ServiceError {
    #[error("no connected chat in pane {0}")]
    UnknownChat(String),
    #[error("the chat has not received its first transcript yet")]
    NotReady,
    #[error("no trusted HTTPS relay is configured for sharing")]
    NoRelays,
    #[error("no active share matches that identifier")]
    UnknownShare,
    #[error(transparent)]
    Sharing(#[from] Error),
    #[error(transparent)]
    Relay(#[from] client::Error),
}

struct LiveShare {
    pane: String,
    history: ShareHistory,
    include_tools: bool,
    created_ms: u64,
    expires_ms: u64,
    grants: Vec<GrantId>,
    relay: String,
    stop: watch::Sender<bool>,
    feed_task: JoinHandle<()>,
    route_task: JoinHandle<()>,
}

impl LiveShare {
    fn shutdown(self) {
        let _ = self.stop.send(true);
        self.feed_task.abort();
        self.route_task.abort();
    }

    fn summary(&self, share: ShareId) -> ShareSummary {
        ShareSummary {
            share: share.0,
            pane: self.pane.clone(),
            history: self.history,
            include_tools: self.include_tools,
            created_ms: self.created_ms,
            expires_ms: self.expires_ms,
            grants: self.grants.len(),
            relay: self.relay.clone(),
        }
    }
}

pub(crate) struct CreateRequest<'a> {
    pub(crate) pane: &'a str,
    pub(crate) history: ShareHistory,
    pub(crate) include_tools: bool,
    pub(crate) lifetime: Duration,
    pub(crate) relay: Option<&'a str>,
}

pub(crate) struct Service {
    owner: Owner,
    live: Mutex<BTreeMap<ShareId, LiveShare>>,
}

impl Service {
    /// Opens the persistent sharing store in `directory` (durably closing any
    /// shares from prior runs) and starts its bounded serial worker. Blocking.
    pub(crate) fn open(directory: &Path) -> Result<Self> {
        Ok(Self {
            owner: Owner::open(directory)?,
            live: Mutex::new(BTreeMap::new()),
        })
    }

    /// Non-secret summaries of currently active shares.
    pub(crate) fn list(&self) -> Vec<ShareSummary> {
        let now = now_ms();
        let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        let ended: Vec<ShareId> = live
            .iter()
            .filter(|(_, entry)| entry.expires_ms <= now || entry.route_task.is_finished())
            .map(|(id, _)| *id)
            .collect();
        for id in ended {
            if let Some(entry) = live.remove(&id) {
                entry.shutdown();
            }
        }
        live.iter().map(|(id, entry)| entry.summary(*id)).collect()
    }

    /// Exact host-side allowlist projection of `pane`'s current transcript,
    /// without creating any share, grant, or relay connection.
    pub(crate) fn preview(
        &self,
        hub: &Hub,
        pane: &str,
        history: ShareHistory,
        include_tools: bool,
    ) -> std::result::Result<SharePreview, ServiceError> {
        let (_, chat) = hub
            .chat_with_endpoint(pane)
            .ok_or_else(|| ServiceError::UnknownChat(pane.to_owned()))?;
        let (transcript, cursor) = chat.base();
        if cursor.is_none() {
            return Err(ServiceError::NotReady);
        }
        project_preview(&transcript, history, include_tools).map_err(ServiceError::Sharing)
    }

    /// Owner-confirmed creation of a share for `pane`, its first revocable
    /// grant, a dedicated relay route, and its copyable v0 HTTPS link.
    pub(crate) async fn create(
        &self,
        hub: &Hub,
        relays: Option<&Relays>,
        request: CreateRequest<'_>,
    ) -> std::result::Result<ShareCreated, ServiceError> {
        let (endpoint_id, chat) = hub
            .chat_with_endpoint(request.pane)
            .ok_or_else(|| ServiceError::UnknownChat(request.pane.to_owned()))?;
        let relays = relays.ok_or(ServiceError::NoRelays)?;
        let relay = relays
            .share_relay(request.relay)
            .ok_or(ServiceError::NoRelays)?;
        let token = relays.token_for(&relay);
        let token_bytes = token
            .as_ref()
            .map_or(&[][..], |token| token.expose().as_bytes());
        let (feed, feed_task) = feed_from_chat(endpoint_id, chat);
        if feed.borrow().is_none() {
            feed_task.abort();
            return Err(ServiceError::NotReady);
        }
        let connection = match client::reach(&relay).await {
            Ok(connection) => connection,
            Err(error) => {
                feed_task.abort();
                return Err(ServiceError::Relay(error));
            }
        };
        self.create_on(
            request.pane,
            feed,
            feed_task,
            &relay,
            connection,
            token_bytes,
            request.history,
            request.include_tools,
            request.lifetime,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn create_on(
        &self,
        pane: &str,
        feed: Feed,
        feed_task: JoinHandle<()>,
        relay: &Endpoint,
        connection: quinn::Connection,
        token: &[u8],
        history: ShareHistory,
        include_tools: bool,
        lifetime: Duration,
    ) -> std::result::Result<ShareCreated, ServiceError> {
        let lifetime = lifetime.min(MAX_LIFETIME);
        let share_history = to_history(history);
        let content = Content {
            tools: include_tools,
        };
        let created_ms = now_ms();
        let share = match self
            .owner
            .create(feed, share_history, content, lifetime)
            .await
        {
            Ok(id) => id,
            Err(error) => {
                feed_task.abort();
                return Err(ServiceError::Sharing(error));
            }
        };
        let invite = match self.owner.invite(share).await {
            Ok(invite) => invite,
            Err(error) => {
                let _ = self.owner.stop(share).await;
                feed_task.abort();
                return Err(ServiceError::Sharing(error));
            }
        };
        let route = match Route::new() {
            Ok(route) => Arc::new(route),
            Err(error) => {
                let _ = self.owner.stop(share).await;
                feed_task.abort();
                return Err(ServiceError::Sharing(error));
            }
        };
        let serving = match Arc::clone(&route)
            .listen(connection, token, self.owner.access(share))
            .await
        {
            Ok(serving) => serving,
            Err(error) => {
                let _ = self.owner.stop(share).await;
                feed_task.abort();
                return Err(ServiceError::Sharing(error));
            }
        };
        let (stop, stopped) = watch::channel(false);
        let route_task = tokio::spawn(async move {
            if let Err(error) = serving.run(stopped).await {
                tracing::debug!(%error, "shared-session relay route ended");
            }
        });
        let offer = match route.offer(invite.grant).await {
            Ok(offer) => offer,
            Err(error) => {
                let _ = stop.send(true);
                route_task.abort();
                feed_task.abort();
                let _ = self.owner.stop(share).await;
                return Err(ServiceError::Sharing(error));
            }
        };
        let encoded = match link::encode(relay, &invite, &offer) {
            Ok(link) => link,
            Err(error) => {
                let _ = stop.send(true);
                route_task.abort();
                feed_task.abort();
                let _ = self.owner.stop(share).await;
                return Err(ServiceError::Sharing(error));
            }
        };
        let entry = LiveShare {
            pane: pane.to_owned(),
            history,
            include_tools,
            created_ms,
            expires_ms: invite.expires_ms,
            grants: vec![invite.grant],
            relay: relay.to_string(),
            stop,
            feed_task,
            route_task,
        };
        let summary = entry.summary(share);
        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(share, entry);
        Ok(ShareCreated {
            share: summary,
            grant: invite.grant.0,
            link: ShareLink::new(encoded.expose_secret().to_owned()),
        })
    }

    /// Revokes one grant on `share`, or stops the entire share when `grant` is
    /// `None` (or when revoking removes the last active grant).
    pub(crate) async fn revoke(
        &self,
        share: ShareId,
        grant: Option<GrantId>,
    ) -> std::result::Result<(), ServiceError> {
        let stop_whole = match grant {
            None => true,
            Some(grant) => {
                self.owner.revoke(share, grant).await.map_err(|error| {
                    if matches!(error, Error::Denied) {
                        ServiceError::UnknownShare
                    } else {
                        ServiceError::Sharing(error)
                    }
                })?;
                let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(entry) = live.get_mut(&share) {
                    entry.grants.retain(|id| *id != grant);
                    entry.grants.is_empty()
                } else {
                    false
                }
            }
        };
        if stop_whole {
            self.owner.stop(share).await.map_err(|error| {
                if matches!(error, Error::Denied) {
                    ServiceError::UnknownShare
                } else {
                    ServiceError::Sharing(error)
                }
            })?;
            let removed = self
                .live
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&share);
            if let Some(entry) = removed {
                entry.shutdown();
            }
        }
        Ok(())
    }

    /// Owner kill switch: stops every share and closes all guest streams.
    pub(crate) async fn stop_all(&self) -> Result<()> {
        let result = self.owner.stop_all().await;
        self.shutdown_routes();
        result
    }

    pub(crate) fn shutdown_routes(&self) {
        let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        for (_, entry) in std::mem::take(&mut *live) {
            entry.shutdown();
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown_routes();
    }
}

fn to_history(history: ShareHistory) -> History {
    match history {
        ShareHistory::All => History::All,
        ShareHistory::FromNow => History::FromNow,
    }
}

/// Runs the exact guest projection filter for the owner's preview dialog.
pub(crate) fn project_preview(
    transcript: &Transcript,
    history: ShareHistory,
    include_tools: bool,
) -> Result<SharePreview> {
    let boundary = match history {
        ShareHistory::All => None,
        ShareHistory::FromNow => transcript.last_entry_id(),
    };
    let content = Content {
        tools: include_tools,
    };
    let update = Projection::new().read(transcript, boundary, content, None)?;
    let projected = match update {
        Update::Snapshot { transcript, .. } => *transcript,
        _ => Transcript::default(),
    };
    let total_entries = projected.entries.len();
    let truncated = projected.truncated || total_entries > MAX_PREVIEW_ENTRIES;
    let start = total_entries.saturating_sub(MAX_PREVIEW_ENTRIES);
    let entries = projected.entries[start..]
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, .. } => Some(PreviewEntry {
                role: PreviewRole::User,
                text: excerpt(text, MAX_PREVIEW_CHARS),
            }),
            EntryKind::Agent { text } => Some(PreviewEntry {
                role: PreviewRole::Agent,
                text: excerpt(text, MAX_PREVIEW_CHARS),
            }),
            EntryKind::Tool(tool) => {
                let mut text = tool.title.clone();
                if let Some(ToolContent::Text { text: body }) = tool.content.first()
                    && !body.is_empty()
                {
                    if !text.is_empty() {
                        text.push_str(": ");
                    }
                    text.push_str(body);
                }
                Some(PreviewEntry {
                    role: PreviewRole::Tool,
                    text: excerpt(&text, MAX_PREVIEW_CHARS),
                })
            }
            _ => None,
        })
        .collect();
    Ok(SharePreview {
        entries,
        truncated,
        total_entries,
    })
}

fn excerpt(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let mut out = String::new();
    for (index, ch) in trimmed.chars().enumerate() {
        if index >= max_chars {
            out.push('…');
            break;
        }
        out.push(if ch == '\r' || ch == '\n' { ' ' } else { ch });
    }
    out
}

/// Bridges a live `Hub` chat into the coalesced `watch::Receiver<Option<Snapshot>>`
/// expected by `sharing::worker::Owner`.
pub(crate) fn feed_from_chat(endpoint: EndpointId, chat: Arc<Chat>) -> (Feed, JoinHandle<()>) {
    let (view, mut events) = chat.subscribe();
    let (mut current, mut current_cursor) = chat.base();
    let initial = current_cursor
        .filter(|_| view.connected && !chat.is_closed())
        .map(|cursor| Snapshot {
            endpoint: endpoint.clone(),
            cursor,
            transcript: Arc::new(current.clone()),
        });
    let (tx, rx) = watch::channel(initial);
    let handle = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(Event::Reset { transcript, cursor }) => {
                    current = (*transcript).clone();
                    current_cursor = Some(cursor);
                    let _ = tx.send(Some(Snapshot {
                        endpoint: endpoint.clone(),
                        cursor,
                        transcript,
                    }));
                }
                Ok(Event::Change { change, cursor }) => {
                    if let Some(prev) = current_cursor
                        && prev.epoch == cursor.epoch
                        && cursor.seq <= prev.seq
                    {
                        continue;
                    }
                    let applied = current_cursor.is_some_and(|prev| prev.epoch == cursor.epoch)
                        && current.apply(&change).is_ok();
                    if applied {
                        current_cursor = Some(cursor);
                    } else {
                        let (base, base_cursor) = chat.base();
                        current = base;
                        current_cursor = base_cursor;
                    }
                    if let Some(cursor) = current_cursor {
                        let _ = tx.send(Some(Snapshot {
                            endpoint: endpoint.clone(),
                            cursor,
                            transcript: Arc::new(current.clone()),
                        }));
                    }
                }
                Ok(Event::Connection) => {
                    if chat.is_closed() {
                        let _ = tx.send(None);
                        break;
                    }
                }
                Ok(Event::Summary) => {}
                Err(RecvError::Lagged(_)) => {
                    let (base, base_cursor) = chat.base();
                    current = base;
                    current_cursor = base_cursor;
                    let snap = current_cursor.map(|cursor| Snapshot {
                        endpoint: endpoint.clone(),
                        cursor,
                        transcript: Arc::new(current.clone()),
                    });
                    let _ = tx.send(snap);
                }
                Err(RecvError::Closed) => {
                    let _ = tx.send(None);
                    break;
                }
            }
        }
    });
    (rx, handle)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
