import Foundation

/// Which chats the phone keeps following while no screen shows them, so
/// opening one shows its latest turn at once rather than after a round
/// trip to the computer: a chat just left, for a while, and the few the
/// lists show busiest or most recently answered. They are followed in the
/// background (`LiveRequest.subscribe`), never marking answers seen, and
/// few enough per computer to leave its socket room for views on screen.
public struct WarmChats<Key: Hashable & Sendable>: Sendable {
    /// One the lists offer: the computer it is on, whether its agent works
    /// or waits on the person, and when it last answered.
    public struct Listed: Sendable {
        public let key: Key
        public let computer: String
        public let busy: Bool
        public let lastAnswer: Date?

        public init(key: Key, computer: String = "", busy: Bool, lastAnswer: Date?) {
            self.key = key
            self.computer = computer
            self.busy = busy
            self.lastAnswer = lastAnswer
        }
    }

    /// How long a chat left stays followed, for a quick look elsewhere,
    /// and how long one the computer stopped is left alone.
    public static var lingers: TimeInterval { 60 }
    /// How many listed chats are followed; each streams its changes.
    public static var listed: Int { 4 }
    /// The most followed on one computer. Its socket follows eight views,
    /// one of them its lists, so this leaves four for screens.
    public static var perComputer: Int { 3 }

    /// When each chat left lingers until.
    private var left: [Key: Date] = [:]
    /// When each chat the computer stopped may be followed again.
    private var stopped: [Key: Date] = [:]

    public init() {}

    /// The last screen showing `key` closed at `now`.
    public mutating func closed(_ key: Key, at now: Date) {
        left[key] = now.addingTimeInterval(Self.lingers)
    }

    /// The computer stopped following `key` at `now`, to make room for a
    /// view on screen; asking again at once would only be refused.
    public mutating func stopped(_ key: Key, at now: Date) {
        left[key] = nil
        stopped[key] = now.addingTimeInterval(Self.lingers)
    }

    /// The chats to follow at `now` among those `listed`: the ones left
    /// that still linger, latest first, then the busiest and most recently
    /// answered, at most `perComputer` on one computer. A chat no longer
    /// listed, as on a forgotten computer, or one the computer stopped
    /// lately, is not followed.
    public mutating func keys(_ listed: [Listed], at now: Date) -> Set<Key> {
        let known = Set(listed.map(\.key))
        left = left.filter { $0.value > now && known.contains($0.key) }
        stopped = stopped.filter { $0.value > now }
        let lingering = listed.filter { left[$0.key] != nil }
            .sorted { left[$0.key, default: now] > left[$1.key, default: now] }
        let ranked = listed.sorted { a, b in
            if a.busy != b.busy { return a.busy }
            return (a.lastAnswer ?? .distantPast) > (b.lastAnswer ?? .distantPast)
        }
        var chosen = Set<Key>()
        var counts: [String: Int] = [:]
        var picked = 0
        for chat in lingering + ranked where stopped[chat.key] == nil && !chosen.contains(chat.key) {
            let lingers = left[chat.key] != nil
            // A full computer's chat leaves its place to another's.
            guard lingers || picked < Self.listed, counts[chat.computer, default: 0] < Self.perComputer else { continue }
            counts[chat.computer, default: 0] += 1
            chosen.insert(chat.key)
            if !lingers { picked += 1 }
        }
        return chosen
    }
}
