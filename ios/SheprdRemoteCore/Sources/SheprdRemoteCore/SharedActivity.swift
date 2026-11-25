import CryptoKit
import Foundation

public struct SharedActivityCard: Codable, Hashable, Sendable {
    public let group: String
    public let card: String

    public func belongs(to secret: String?) -> Bool {
        guard let secret else { return false }
        return group == SHA256.hash(data: Data(secret.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    public static func slot(group: String, computer: Fingerprint) -> String {
        SHA256.hash(data: Data((group + computer.hex).utf8)).prefix(16)
            .map { String(format: "%02x", $0) }.joined()
    }
}

public struct SharedActivitySnapshot: Codable, Hashable, Sendable {
    struct Label: Codable, Hashable, Sendable {
        let slot: String
        let sealed: String
    }

    let slot: String
    let sealed: String
    let working: Int
    let waiting: Int
    let done: Int
    let total: Int
    let latest: Label?

    public func opened(
        group: String, keys: [SealedChatActivity.Key], generic: Bool
    ) -> SealedChatActivity.Opened? {
        func owner(_ slot: String) -> SealedChatActivity.Key? {
            keys.first { SharedActivityCard.slot(group: group, computer: $0.computer) == slot }
        }
        guard done >= 0, done <= total, let key = owner(slot),
              var state: ChatActivityState = SealedChatActivity.open(sealed, with: key),
              let facts = state.focus, facts.fingerprint == key.computer,
              !generic || !facts.namesChat else { return nil }
        var label: String?
        if !generic, let latest, let source = owner(latest.slot) {
            label = SealedChatActivity.open(latest.sealed, with: source)
        }
        state.working = working
        state.waiting = waiting
        state.agents = .init(done: done, total: total, latest: label)
        return .init(facts: facts, state: state)
    }
}
