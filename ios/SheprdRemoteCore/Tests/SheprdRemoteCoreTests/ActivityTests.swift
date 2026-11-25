import XCTest
@testable import SheprdRemoteCore

/// The fixtures are the payloads the daemon's tests produce
/// (`crates/sheprd-remote-control/src/apns/tests/payload.rs`).
final class ActivityTests: XCTestCase {
    private func aps(_ name: String) throws -> [String: Any] {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        let root = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        return try XCTUnwrap(root?["aps"] as? [String: Any])
    }

    private func decode<T: Decodable>(_ type: T.Type, _ object: Any?) throws -> T {
        let data = try JSONSerialization.data(withJSONObject: try XCTUnwrap(object))
        return try JSONDecoder().decode(type, from: data)
    }

    func testStartDecodesAttributesAndState() throws {
        let aps = try aps("start")
        XCTAssertEqual(aps["event"] as? String, "start")
        XCTAssertEqual(aps["attributes-type"] as? String, "ChatActivityAttributes")
        let facts = try decode(ChatActivityFacts.self, aps["attributes"])
        XCTAssertEqual(
            facts,
            ChatActivityFacts(
                computer: "studio",
                computerId: String(repeating: "AB", count: 32),
                workspace: "herdr-gpui",
                chatTitle: "remote plan",
                agent: "Claude",
                pane: "w1-p3"
            )
        )
        XCTAssertEqual(facts.place, "herdr-gpui \u{b7} remote plan")
        XCTAssertEqual(facts.fingerprint, Fingerprint(String(repeating: "AB", count: 32)))
        XCTAssertEqual(facts.link.fingerprint, facts.fingerprint)
        let state = try decode(ChatActivityState.self, aps["content-state"])
        XCTAssertEqual(state.phase, .working)
        XCTAssertEqual(state.startedAt, Date(timeIntervalSince1970: 1_799_999_990))
        XCTAssertEqual(state.working, 1)
        XCTAssertNil(state.planFraction)
        XCTAssertEqual(state.headline, "Working")
    }

    func testDelegatedAgentsRoundTripWithTheirLatestOutcome() throws {
        let state = try JSONDecoder().decode(ChatActivityState.self, from: Data(
            #"{"phase":"working","step":"Review","startedAt":1,"agents":{"done":1,"total":5,"latest":"Finished · Parser"}}"#.utf8
        ))
        XCTAssertEqual(state.agents?.fraction, 0.2)
        XCTAssertEqual(state.agents?.label, "1 of 5 agents done")
        XCTAssertEqual(state.agents?.latest, "Finished · Parser")
        XCTAssertEqual(try JSONDecoder().decode(ChatActivityState.self, from: JSONEncoder().encode(state)), state)
    }

    func testGroupingDefaultsToOneAndPersistsIndependentlyPerComputer() throws {
        let suite = UUID().uuidString
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        let first = try XCTUnwrap(Fingerprint(String(repeating: "AA", count: 32)))
        let second = try XCTUnwrap(Fingerprint(String(repeating: "BB", count: 32)))
        XCTAssertTrue(ActivityGrouping.single(for: first, in: defaults))
        defaults.set(true, forKey: ActivityGrouping.key(for: first))
        XCTAssertFalse(ActivityGrouping.single(for: first, in: try XCTUnwrap(UserDefaults(suiteName: suite))))
        XCTAssertTrue(ActivityGrouping.single(for: second, in: defaults))
        defaults.set(true, forKey: ActivityGrouping.defaultKey)
        XCTAssertFalse(ActivityGrouping.single(for: second, in: defaults))
        defaults.set(false, forKey: ActivityGrouping.key(for: first))
        XCTAssertTrue(ActivityGrouping.single(for: first, in: defaults))
        defaults.removeObject(forKey: ActivityGrouping.key(for: first))
        XCTAssertFalse(ActivityGrouping.single(for: first, in: defaults))
    }

    func testOnlySharedStartsFromBeforeAnExplicitResetAreRetired() {
        let epoch = UUID()
        XCTAssertFalse(ActivityGrouping.isRetiredHostCard("w1:p1", since: epoch))
        XCTAssertFalse(ActivityGrouping.isRetiredHostCard("host-activity/old", since: nil))
        XCTAssertTrue(ActivityGrouping.isRetiredHostCard("host-activity/old", since: epoch))
        let new = "host-activity/" + epoch.uuidString.lowercased() + "/new"
        XCTAssertFalse(ActivityGrouping.isRetiredHostCard(new, since: epoch))
    }

    func testHostCardRoutesItsTokenAndQuestionIndependently() throws {
        let start = try aps("start")
        var attributes = try decode(ChatActivityFacts.self, start["attributes"])
        var state = try decode(ChatActivityState.self, aps("update")["content-state"])
        var focus = attributes
        focus.pane = "second"
        attributes.pane = "host-activity/test"
        state.focus = focus
        let content = try JSONDecoder().decode(ChatActivityState.self, from: JSONEncoder().encode(state))
        let opened = try XCTUnwrap(SealedChatActivity.open(attributes: .plain(attributes), state: .plain(content), keys: [], allowsPlain: true))
        XCTAssertEqual(opened.facts.link.pane, "second")
        XCTAssertEqual(opened.state.permission, 7)
        XCTAssertEqual(SealedChatActivity.facts(.plain(attributes), keys: [])?.0.pane, "host-activity/test")
        focus.computerId = String(repeating: "BB", count: 32)
        state.focus = focus
        XCTAssertEqual(SealedChatActivity.Opened(facts: attributes, state: state).facts.computerId, attributes.computerId)
    }

    func testQuestionUpdateDecodes() throws {
        let state = try decode(ChatActivityState.self, aps("update")["content-state"])
        XCTAssertEqual(state.phase, .question)
        XCTAssertEqual(state.question, "Run just ci")
        XCTAssertEqual(state.planFraction, 0.5)
        XCTAssertEqual(state.planLabel, "Step 3 of 4")
        XCTAssertEqual(state.waiting, 1)
        XCTAssertEqual(state.headline, "Needs an answer")
        // What the lock screen's answer buttons and step stack need.
        XCTAssertEqual(state.permission, 7)
        XCTAssertEqual(state.options.map(\.id), ["allow", "reject"])
        XCTAssertEqual(state.options.map(\.kind), [.allowOnce, .rejectOnce])
        XCTAssertEqual(state.previous, ["Read lib.rs", "Edit lib.rs"])
    }

    func testEndDecodes() throws {
        let aps = try aps("end")
        XCTAssertEqual(aps["dismissal-date"] as? Int, 1_800_001_262)
        let state = try decode(ChatActivityState.self, aps["content-state"])
        XCTAssertEqual(state.phase, .finished)
        XCTAssertEqual(state.summary, "Worked 6m 2s")
        XCTAssertEqual(state.headline, "Finished")
    }

    func testStateRoundTripsAsUnixSeconds() throws {
        let state = ChatActivityState(
            phase: .failed,
            step: "Run",
            startedAt: Date(timeIntervalSince1970: 1_800_000_000),
            summary: "Claude stopped"
        )
        let data = try JSONEncoder().encode(state)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(object?["startedAt"] as? Double, 1_800_000_000)
        XCTAssertNil(object?["cost"])
        XCTAssertEqual(try JSONDecoder().decode(ChatActivityState.self, from: data), state)
    }

    func testMissingCountsDefaultToZero() throws {
        let state = try JSONDecoder().decode(
            ChatActivityState.self,
            from: Data(#"{"phase":"working","step":"x","startedAt":1}"#.utf8)
        )
        XCTAssertEqual(state.working, 0)
        XCTAssertEqual(state.waiting, 0)
    }

    func testAnOlderDaemonsCostIsIgnored() throws {
        let state = try JSONDecoder().decode(
            ChatActivityState.self,
            from: Data(#"{"phase":"working","step":"x","startedAt":1,"cost":"$0.34"}"#.utf8)
        )
        XCTAssertEqual(state.step, "x")
        let data = try JSONEncoder().encode(state)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertNil(object?["cost"])
    }

    func testChatLinkRoundTripsAndEscapesThePane() throws {
        let id = String(repeating: "0F", count: 32)
        let link = ChatLink(computer: "Fabien's Mac", computerId: id, pane: "w1 p/3")
        XCTAssertEqual(ChatLink(url: link.url), link)
        XCTAssertEqual(link.fingerprint?.hex, id)
        XCTAssertEqual(link.url.scheme, "herdr-remote")
        XCTAssertEqual(
            link.page(origin: "https://192.168.1.20:7443")?.absoluteString,
            "https://192.168.1.20:7443/chat/w1%20p%2F3"
        )
        XCTAssertNil(ChatLink(url: try XCTUnwrap(URL(string: "https://example.com/chat?pane=x&computer=y"))))
        XCTAssertNil(ChatLink(url: try XCTUnwrap(URL(string: "herdr-remote://chat?computer=y&id=\(id)"))))
        // The name alone never names a computer.
        XCTAssertNil(ChatLink(url: try XCTUnwrap(URL(string: "herdr-remote://chat?computer=y&pane=p"))))
    }

    func testATerminalLinkOpensTheTerminal() throws {
        let id = String(repeating: "0F", count: 32)
        let link = ChatLink(computer: "Mac", computerId: id, pane: "t1-p1", terminal: true)
        let read = try XCTUnwrap(ChatLink(url: link.url))
        XCTAssertTrue(read.terminal)
        XCTAssertEqual(read, link)
        XCTAssertEqual(link.page(origin: "https://mac:7443")?.absoluteString, "https://mac:7443/term/t1-p1")
        XCTAssertFalse(try XCTUnwrap(ChatLink(url: ChatLink(computer: "Mac", computerId: id, pane: "p").url)).terminal)
    }

    func testATerminalActivitySaysSoAndAChatNeedNot() throws {
        let id = String(repeating: "0F", count: 32)
        let chat = #"{"computer":"Mac","computerId":"\#(id)","workspace":"w","chatTitle":"c","agent":"Claude","pane":"p"}"#
        let facts = try JSONDecoder().decode(ChatActivityFacts.self, from: Data(chat.utf8))
        XCTAssertFalse(facts.link.terminal)
        let terminal = chat.replacingOccurrences(of: #""pane":"p""#, with: #""pane":"p","terminal":true"#)
        XCTAssertTrue(try JSONDecoder().decode(ChatActivityFacts.self, from: Data(terminal.utf8)).link.terminal)
    }
}
