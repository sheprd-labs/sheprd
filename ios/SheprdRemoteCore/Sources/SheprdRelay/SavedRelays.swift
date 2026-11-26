import Foundation
import SheprdRemoteCore
import SheprdTunnel

/// The relays this phone keeps for later: the ones its computers named
/// while it reached them directly, and the ones typed in Settings.
///
/// Only typed relays are tried for every computer, after the relays that
/// computer named itself, so a computer that moved to a new relay is still
/// reached once the user adds it here. A relay a computer named is dialed
/// for that computer only: dialing another computer there would tell
/// whoever runs it that computer's route, and when this phone reaches it,
/// though the user never chose that relay for it. Addresses are not
/// secrets: the phone's keys for a computer stay in that computer's
/// Keychain record.
public struct SavedRelays: Codable, Equatable, Sendable {
    /// The most kept: each is one more dial a lost computer costs.
    public static let limit = 16

    public private(set) var relays: [RelayEndpoint]
    /// Those of `relays` the user typed, tried for every computer.
    public private(set) var typed: Set<RelayEndpoint> = []
    /// Relays a computer named that the user removed: not learned again,
    /// and never dialed, not even for the computers that name them, until
    /// the user types one back.
    public private(set) var removed: Set<RelayEndpoint> = []
    /// Read from a version that did not say which were typed, until
    /// `settle` decides. Kept when saved, so a launch that could not read
    /// the paired computers leaves the decision to a later one; until
    /// then no saved relay is tried beyond the computers that named it.
    private var unsettled = false
    /// While unsettled, the relays the user added or stopped sharing
    /// since: `settle` leaves those choices as they are.
    private var decided: Set<RelayEndpoint> = []

    public init(_ relays: [RelayEndpoint] = []) {
        self.relays = []
        learn(relays)
    }

    /// The typed relays, in the order kept: what every computer may be
    /// dialed on beyond its own.
    public var forEveryComputer: [RelayEndpoint] {
        relays.filter(typed.contains)
    }

    /// What dialing a computer needs from this list.
    public var dialing: RelayDialing {
        RelayDialing(typed: forEveryComputer, removed: removed)
    }

    /// What became of an address typed in Settings.
    public enum Added: Equatable, Sendable {
        case added(RelayEndpoint)
        /// Already kept.
        case known
        /// Not a relay's address.
        case invalid
        /// `limit` are kept already.
        case full
    }

    /// Keeps the relay at `text`, as typed.
    public mutating func add(_ text: String) -> Added {
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty, let endpoint = RelayEndpoint(text) else {
            return .invalid
        }
        if relays.contains(endpoint) {
            // Typing a relay a computer named makes it the user's choice
            // for every computer.
            decide(endpoint)
            return typed.insert(endpoint).inserted ? .added(endpoint) : .known
        }
        guard relays.count < Self.limit else { return .full }
        relays.append(endpoint)
        typed.insert(endpoint)
        // Typing a removed relay back is the user taking it again; an add
        // refused leaves it removed.
        removed.remove(endpoint)
        decide(endpoint)
        return .added(endpoint)
    }

    /// Keeps the relays a computer named, up to `limit`. True when any is
    /// new.
    @discardableResult
    public mutating func learn(_ endpoints: [RelayEndpoint]) -> Bool {
        var changed = false
        for endpoint in endpoints where !relays.contains(endpoint) && !removed.contains(endpoint) && relays.count < Self.limit {
            relays.append(endpoint)
            changed = true
        }
        return changed
    }

    public mutating func remove(_ endpoint: RelayEndpoint) {
        relays.removeAll { $0 == endpoint }
        typed.remove(endpoint)
    }

    /// Removes a relay a computer named, for good: it is not learned
    /// again, nor dialed for that computer, until the user types it back.
    public mutating func drop(_ endpoint: RelayEndpoint) {
        remove(endpoint)
        removed.insert(endpoint)
        decide(endpoint)
    }

    /// Forgets the removals of relays no paired computer (`named`) names
    /// any more, so the list of them stays as short as the computers'.
    public mutating func keepRemoved(named: Set<RelayEndpoint>) {
        removed.formIntersection(named)
    }

    /// Stops trying `endpoint` for every computer, keeping it for the
    /// computers that name it.
    public mutating func unshare(_ endpoint: RelayEndpoint) {
        typed.remove(endpoint)
        decide(endpoint)
    }

    /// Records a choice the user made before an older list was settled.
    private mutating func decide(_ endpoint: RelayEndpoint) {
        if unsettled { decided.insert(endpoint) }
    }

    /// Decides, once, for a list kept before typed relays were told apart:
    /// a relay one of the paired computers names (`named`) is that
    /// computer's; any other was typed, or named by a computer since
    /// removed, and stays for every computer as before, where Settings
    /// shows it and the user can remove it.
    public mutating func settle(named: Set<RelayEndpoint>) {
        guard unsettled else { return }
        unsettled = false
        let undecided = relays.filter { !decided.contains($0) }
        typed.formUnion(undecided.filter { !named.contains($0) })
        decided = []
    }

    private enum CodingKeys: String, CodingKey {
        case decided, relays, removed, typed, unsettled
    }

    public init(from decoder: Decoder) throws {
        // One by one: an address a later version writes and this one
        // cannot read costs that relay, not the list.
        let read = { (texts: [String]) in texts.compactMap(RelayEndpoint.init) }
        if let texts = try? decoder.singleValueContainer().decode([String].self) {
            self.init(read(texts))
            unsettled = true
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(read(try container.decode([String].self, forKey: .relays)))
        let typed = read(try container.decodeIfPresent([String].self, forKey: .typed) ?? [])
        self.typed = Set(typed.filter(relays.contains))
        removed = Set(read(try container.decodeIfPresent([String].self, forKey: .removed) ?? []))
        unsettled = try container.decodeIfPresent(Bool.self, forKey: .unsettled) ?? false
        if unsettled {
            let decided = read(try container.decodeIfPresent([String].self, forKey: .decided) ?? [])
            self.decided = Set(decided.filter(relays.contains))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(relays.map(\.description), forKey: .relays)
        try container.encode(forEveryComputer.map(\.description), forKey: .typed)
        if !removed.isEmpty {
            try container.encode(removed.map(\.description).sorted(), forKey: .removed)
        }
        if unsettled {
            try container.encode(true, forKey: .unsettled)
            try container.encode(relays.filter(decided.contains).map(\.description), forKey: .decided)
        }
    }
}

/// The user's choices a computer's relays are dialed with: the typed
/// relays, tried for every computer after its own, and the removed ones,
/// never tried.
public struct RelayDialing: Equatable, Sendable {
    public var typed: [RelayEndpoint]
    public var removed: Set<RelayEndpoint>

    public init(typed: [RelayEndpoint] = [], removed: Set<RelayEndpoint> = []) {
        self.typed = typed
        self.removed = removed
    }
}

/// Whether a relay answers, as its `/v1/info` says over HTTPS. Shown in
/// Settings; nothing is decided on it, since a relay that is down now may
/// be back when a computer is dialed.
public enum RelayReach: Equatable, Sendable {
    case reachable
    /// It answers, but not over WebSockets, which this phone dials with.
    case noWebSocket
    /// Something answers there that is not a relay this app speaks to.
    case notARelay
    case unreachable
    case banned

    struct Info: Decodable {
        let `protocol`: Int
        let websocket: Bool?
    }

    /// What an answer to `GET /v1/info` means.
    public static func read(status: Int, body: Data) -> RelayReach {
        outcome(status: status, body: body).reach
    }

    /// The most of an answer that is read: a relay's `/v1/info` is a few
    /// hundred bytes.
    static let maxInfo = 16 * 1024

    /// Asks `endpoint` whether it is a relay, waiting at most ten seconds.
    /// The answer is read as it arrives and dropped once it is longer than
    /// `maxInfo`, so whatever a saved address serves, it costs this phone
    /// no more memory than that.
    public static func probe(_ endpoint: RelayEndpoint, session: URLSession = .shared) async -> RelayReach {
        await diagnose(endpoint, session: session).reach
    }

    /// What `bytes` carries, or nil once it carries more than `limit`: the
    /// rest is never read.
    static func bounded<Bytes: AsyncSequence>(_ bytes: Bytes, limit: Int) async throws -> Data?
        where Bytes.Element == UInt8
    {
        var body = Data()
        for try await byte in bytes {
            guard body.count < limit else { return nil }
            body.append(byte)
        }
        return body
    }
}
