import Foundation
import Testing
@testable import SheprdRemoteCore

struct WarmChatsTests {
    private let now = Date(timeIntervalSince1970: 1_000_000)

    /// Each on a computer of its own unless `on` names one.
    private func listed(_ key: String, on computer: String? = nil, busy: Bool = false, answered: TimeInterval? = nil) -> WarmChats<String>.Listed {
        .init(key: key, computer: computer ?? key, busy: busy, lastAnswer: answered.map { now.addingTimeInterval(-$0) })
    }

    @Test func busyChatsComeFirstThenTheMostRecentlyAnswered() {
        var warm = WarmChats<String>()
        let many = (0..<WarmChats<String>.listed + 2).map { listed("old\($0)", answered: 1000 + Double($0)) }
        let keys = warm.keys(many + [listed("busy", busy: true), listed("recent", answered: 5)], at: now)
        #expect(keys.count == WarmChats<String>.listed)
        #expect(keys.contains("busy"))
        #expect(keys.contains("recent"))
        #expect(!keys.contains("old\(WarmChats<String>.listed + 1)"))
    }

    @Test func aChatLeftLingersBeyondTheListedFew() {
        var warm = WarmChats<String>()
        let busy = (0..<WarmChats<String>.listed).map { listed("busy\($0)", busy: true) }
        warm.closed("quiet", at: now)
        let soon = now.addingTimeInterval(WarmChats<String>.lingers - 1)
        #expect(warm.keys(busy + [listed("quiet")], at: soon).contains("quiet"))
    }

    @Test func aChatLeftStopsLingering() {
        var warm = WarmChats<String>()
        let busy = (0..<WarmChats<String>.listed).map { listed("busy\($0)", busy: true) }
        warm.closed("quiet", at: now)
        let later = now.addingTimeInterval(WarmChats<String>.lingers)
        #expect(!warm.keys(busy + [listed("quiet")], at: later).contains("quiet"))
    }

    @Test func oneComputerKeepsRoomForScreens() {
        var warm = WarmChats<String>()
        warm.closed("left", at: now)
        let busy = (0..<WarmChats<String>.listed).map { listed("busy\($0)", on: "mac", busy: true) }
        let keys = warm.keys(busy + [listed("left", on: "mac"), listed("other", busy: true)], at: now)
        let onMac = keys.filter { $0 != "other" }
        #expect(onMac.count == WarmChats<String>.perComputer)
        #expect(onMac.contains("left"), "the chat just left comes first")
        #expect(keys.contains("other"))
    }

    @Test func aChatTheComputerStoppedIsLeftAlone() {
        var warm = WarmChats<String>()
        warm.closed("quiet", at: now)
        warm.stopped("quiet", at: now)
        #expect(!warm.keys([listed("quiet", busy: true)], at: now).contains("quiet"))
        let later = now.addingTimeInterval(WarmChats<String>.lingers)
        #expect(warm.keys([listed("quiet", busy: true)], at: later).contains("quiet"))
    }

    @Test func aChatNoLongerListedIsNotFollowed() {
        var warm = WarmChats<String>()
        warm.closed("gone", at: now)
        #expect(warm.keys([listed("other")], at: now) == ["other"])
    }
}
