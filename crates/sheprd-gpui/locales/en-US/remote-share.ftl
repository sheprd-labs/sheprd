# Share this session dialog (Phase 2 read-only session sharing).

remote-share-title = Share this session
remote-share-subtitle = Read-only · Agent stays on this machine
remote-share-off = Remote control is off
remote-share-off-detail = Turn on Remote control in Settings to create expiring read-only links over a relay.
remote-share-unavailable = Session sharing is not available in this window.
remote-share-open-settings = Open Remote control settings

remote-share-permissions-label = Access
remote-share-view-only = View only
remote-share-view-only-detail = Guests can read permitted messages. No prompts, approvals, files, or terminals.

remote-share-history-label = Conversation history
remote-share-history-from-now = From this point
remote-share-history-all = Entire conversation

remote-share-tools-label = Tool output
remote-share-tools-off = Excluded
remote-share-tools-on = Include tool text
remote-share-tools-note = File paths, diffs, images, and terminal handles always stay excluded.

remote-share-expiry-label = Link expires in
remote-share-expiry-15m = 15 min
remote-share-expiry-30m = 30 min
remote-share-expiry-1h = 1 hour

remote-share-relay-label = Relay
remote-share-relay-none = No relay listening. Configure a relay in Settings > Remote control to create a link.

remote-share-preview-label = Guest preview
remote-share-preview-loading = Loading guest preview…
remote-share-preview-empty-from-now = No prior messages will be shared. Guests only see messages sent after the link is created.
remote-share-preview-empty = No shareable messages yet.
remote-share-preview-count = { $count ->
    [one] { $count } message visible to guests
   *[other] { $count } messages visible to guests
}
remote-share-preview-warning = Selected text is shared as written and is not automatically redacted.
remote-share-preview-role-user = You
remote-share-preview-role-agent = Agent
remote-share-preview-role-tool = Tool

remote-share-create = Create invitation link
remote-share-creating = Creating link…
remote-share-created-label = One-time invitation link
remote-share-created-note = Anyone with this link can view this session until it expires.
remote-share-copy = Copy link
remote-share-copied = Copied

remote-share-active-title = Active shares for this chat
remote-share-active-row = { $history } · { $tools } · { $relay }
remote-share-stop = Stop sharing
remote-share-stopping = Stopping…
remote-share-close = Close
remote-share-retry = Retry
