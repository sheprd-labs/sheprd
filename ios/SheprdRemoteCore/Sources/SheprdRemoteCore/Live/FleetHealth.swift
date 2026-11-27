import Foundation

/// Health summary and formatting for Settings > Computers, keeping the
/// fleet counts and row labels deterministic and testable outside SwiftUI.
public enum FleetHealth {
    /// How an online computer is currently reached.
    public enum RouteKind: Equatable, Hashable, Sendable {
        case local
        case tailscale
        case relay
        case online

        public var label: String {
            switch self {
            case .local: "Local"
            case .tailscale: "Tailscale"
            case .relay: "Relay"
            case .online: "Online"
            }
        }
    }

    /// One paired computer's contribution to the Settings health summary.
    public struct Entry: Equatable, Sendable {
        public var status: LinkStatus
        public var stale: Bool
        public var needsUpdate: Bool
        public var working: Int
        public var waiting: Int

        public init(
            status: LinkStatus,
            stale: Bool = false,
            needsUpdate: Bool = false,
            working: Int = 0,
            waiting: Int = 0
        ) {
            self.status = status
            self.stale = stale
            self.needsUpdate = needsUpdate
            self.working = working
            self.waiting = waiting
        }

        public var isOnline: Bool {
            if case .online = status { return true }
            return false
        }
    }

    /// Aggregate fleet counts shown at the top of Settings > Computers and
    /// beside the Computers row in Settings.
    public struct Summary: Equatable, Sendable {
        public var online: Int
        public var offline: Int
        /// Agents working across online computers with a fresh snapshot;
        /// offline and stale cached snapshots are excluded.
        public var working: Int
        /// Agents waiting for you across online computers with a fresh snapshot.
        public var waiting: Int

        public var total: Int { online + offline }

        public init(entries: [Entry]) {
            var online = 0
            var offline = 0
            var working = 0
            var waiting = 0
            for entry in entries {
                if entry.isOnline {
                    online += 1
                    if !entry.stale {
                        working += max(entry.working, 0)
                        waiting += max(entry.waiting, 0)
                    }
                } else {
                    offline += 1
                }
            }
            self.online = online
            self.offline = offline
            self.working = working
            self.waiting = waiting
        }

        /// Compact summary for the Settings hub row ("Computers").
        public var settingsValue: String {
            guard total > 0 else { return "0" }
            if offline == 0 { return "\(online) online" }
            return "\(online) of \(total) online"
        }
    }

    /// Classifies an online link's active route. A relay proxy runs on a
    /// local address, so `throughRelay` takes precedence over `origin`.
    public static func routeKind(origin: String, throughRelay: Bool) -> RouteKind {
        if throughRelay { return .relay }
        switch OriginKind(origin: origin) {
        case .localNetwork, .localName: return .local
        case .tailscale: return .tailscale
        case .other, .loopback: return .online
        }
    }

    /// Normalizes a daemon release string for display ("0.15.2" -> "v0.15.2").
    public static func versionLabel(_ version: String?) -> String? {
        guard let trimmed = version?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmed.isEmpty
        else { return nil }
        if trimmed.first == "v" || trimmed.first == "V" {
            let rest = trimmed.dropFirst()
            return rest.isEmpty ? nil : "v\(rest)"
        }
        return "v\(trimmed)"
    }

    /// Compact relative or calendar time for when an offline computer was
    /// last heard from.
    public static func lastSeen(_ date: Date, now: Date) -> String {
        let seconds = now.timeIntervalSince(date)
        if seconds < 60 { return "just now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m ago" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h ago" }
        return date.formatted(.dateTime.month(.abbreviated).day())
    }

    /// Minimum gap between persisted last-seen writes during steady load-only
    /// updates so the cache stays current without writing every few seconds.
    public static let lastSeenPersistInterval: TimeInterval = 60

    /// Whether a load-only update should persist `last-seen.json` now.
    public static func shouldPersistLastSeen(
        lastSaved: Date?,
        now: Date,
        interval: TimeInterval = lastSeenPersistInterval
    ) -> Bool {
        guard let lastSaved else { return true }
        let elapsed = now.timeIntervalSince(lastSaved)
        return elapsed >= interval || elapsed < 0
    }

    /// Detail line for an offline computer: last-seen time and configured
    /// reach routes, joined with a middle dot.
    public static func offlineDetail(lastSeen: Date?, reach: String?, now: Date) -> String? {
        let seen = lastSeen.map { "Last seen \(Self.lastSeen($0, now: now))" }
        let parts = [seen, reach].compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}
