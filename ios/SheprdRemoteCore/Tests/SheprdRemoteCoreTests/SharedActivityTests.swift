import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct SharedActivityTests {
    @Test func onlyHostsFollowingTheSharedDefaultParticipate() throws {
        let suite = UUID().uuidString
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let owner = SealedChatActivityTests.owner
        #expect(!ActivityGrouping.shared(for: owner, in: defaults))
        defaults.set(true, forKey: ActivityGrouping.defaultKey)
        defaults.set(true, forKey: ActivityGrouping.allHostsKey)
        #expect(ActivityGrouping.shared(for: owner, in: defaults))
        #expect(ActivityGrouping.single(for: owner, in: defaults))
        for multiple in [false, true] {
            defaults.set(multiple, forKey: ActivityGrouping.key(for: owner))
            #expect(!ActivityGrouping.shared(for: owner, in: defaults))
            #expect(ActivityGrouping.single(for: owner, in: defaults) != multiple)
        }
    }

    @Test func eachFocusAndFinishedLabelUsesItsOwnPairing() throws {
        let group = "local-generation"
        let a = SealedChatActivityTests.owner
        let b = SealedChatActivityTests.other
        let aKey = P256.KeyAgreement.PrivateKey()
        let bKey = P256.KeyAgreement.PrivateKey()
        let keys: [SealedChatActivity.Key] = [
            .init(computer: a, device: aKey.publicKey, agree: aKey.sharedSecretFromKeyAgreement),
            .init(computer: b, device: bKey.publicKey, agree: bKey.sharedSecretFromKeyAgreement),
        ]
        func seal(_ value: some Encodable, _ key: P256.KeyAgreement.PrivateKey) throws -> String {
            try Base64URL.encode(NotificationSeal.seal(JSONEncoder().encode(value), to: key.publicKey, info: SealedChatActivity.info))
        }
        let facts = ChatActivityFacts(computer: "B", computerId: b.hex, workspace: "repo", chatTitle: "Fix", agent: "Agent", pane: "w1-p3")
        var state = ChatActivityState(phase: .question, step: "waiting", startedAt: .now, question: "Proceed?", permission: 7)
        state.focus = facts
        func snapshot(_ owner: Fingerprint) throws -> SharedActivitySnapshot {
            try .init(slot: SharedActivityCard.slot(group: group, computer: owner), sealed: seal(state, bKey),
                      working: 2, waiting: 1, done: 1, total: 2, latest: .init(slot: SharedActivityCard.slot(group: group, computer: a), sealed: seal("Finished · Build", aKey)))
        }
        let good = try snapshot(b)
        let opened = try #require(good.opened(group: group, keys: keys, generic: false))
        #expect(opened.facts.link == facts.link)
        #expect(opened.state.permission == 7)
        #expect(opened.state.agents?.latest == "Finished · Build")
        #expect(good.opened(group: group, keys: keys, generic: true) == nil)
        #expect(good.opened(group: "retired", keys: keys, generic: false) == nil)
        #expect(try snapshot(a).opened(group: group, keys: keys, generic: false) == nil)
        state.focus?.computerId = a.hex
        #expect(try snapshot(b).opened(group: group, keys: keys, generic: false) == nil)
    }
}
