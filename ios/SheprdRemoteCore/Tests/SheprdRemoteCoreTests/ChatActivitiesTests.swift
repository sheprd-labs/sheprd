import Foundation
import Testing
@testable import SheprdRemoteCore

/// Sub-agents, shells, and monitors on the phone: decoded as the daemon
/// writes them, reduced as the Rust reducer does, and grouped for the
/// chat's activity panel.
struct ChatActivitiesTests {
    func activity(_ id: String, _ kind: AgentActivityKind = .subagent, state: AgentActivityState = .running, finished: UInt64? = nil) -> AgentActivity {
        AgentActivity(callId: id, kind: kind, label: id, startedMs: 1, state: state, finishedMs: finished)
    }

    @Test func decodesTheDaemonsActivityChanges() throws {
        guard case let .chatChanges(pane, changes) = try WireTests.event("chat_activity") else {
            Issue.record("not chat changes")
            return
        }
        #expect(pane == "p1")
        var transcript = Transcript(agent: "Claude")
        for step in changes { try transcript.apply(step.change) }
        let running = try #require(transcript.activities.first)
        #expect(running.kind == .subagent)
        #expect(running.isRunning)
        #expect(running.stoppable)
        #expect(running.detail == "Reading crates/sheprd-acp/src/main.rs")
        let ended = transcript.activities[1]
        #expect(ended.kind == .shell)
        #expect(ended.state == .failed)
        #expect(ended.finishedMs == 1_770_000_060_000)
        #expect(!ended.stoppable)
        #expect(transcript.harnesses?.available == ["claude", "codex"])
        #expect(transcript.runningActivities == 1)
    }

    @Test func unknownKindsAndStatesFallBack() throws {
        let json = #"{"call_id":"a","kind":"robot","label":"x","started_ms":1,"state":"paused"}"#
        let activity = try JSONDecoder().decode(AgentActivity.self, from: Data(json.utf8))
        #expect(activity.kind == .other)
        #expect(activity.kind.section == .shell)
        #expect(activity.state == .finished)
    }

    @Test func aChangeReplacesTheActivityWithItsCallId() throws {
        var transcript = Transcript(agent: "Claude")
        try transcript.apply(.activity(activity("a")))
        try transcript.apply(.activity(activity("a", state: .finished, finished: 5)))
        #expect(transcript.activities.count == 1)
        #expect(transcript.activities[0].state == .finished)
    }

    @Test func aFullListDropsTheOldestEndedOrRefuses() throws {
        var transcript = Transcript(agent: "Claude")
        try transcript.apply(.activity(activity("ended", state: .finished, finished: 2)))
        for index in 1..<AgentActivity.limit { try transcript.apply(.activity(activity("run\(index)"))) }
        try transcript.apply(.activity(activity("new")))
        #expect(transcript.activities.count == AgentActivity.limit)
        #expect(!transcript.activities.contains { $0.callId == "ended" })
        #expect(throws: ChangeError.activityLimit) { try transcript.apply(.activity(activity("more"))) }
    }

    @Test func overlongTextIsRefused() {
        var transcript = Transcript(agent: "Claude")
        var long = activity("a")
        long.label = String(repeating: "é", count: AgentActivity.textLimit / 2 + 1)
        #expect(throws: ChangeError.activityText) { try transcript.apply(.activity(long)) }
        #expect(transcript.activities.isEmpty)
    }

    @Test func groupsListRunningInOrderThenEndedNewestFirst() throws {
        var transcript = Transcript(agent: "Claude")
        transcript.activities = [
            activity("old", state: .finished, finished: 10),
            activity("first"),
            activity("shell", .shell),
            activity("new", state: .killed, finished: 20),
            activity("second"),
            activity("odd", .other),
        ]
        let agents = transcript.activityGroup(.subagent)
        #expect(agents.running.map(\.callId) == ["first", "second"])
        #expect(agents.ended.map(\.callId) == ["new", "old"])
        #expect(transcript.activityGroup(.shell).running.map(\.callId) == ["shell", "odd"])
        #expect(transcript.activityGroup(.monitor).isEmpty)
    }

    @Test func changedFilesKeepTheLatestDiffInFirstOrder() throws {
        var transcript = Transcript(agent: "Claude")
        #expect(!transcript.hasActivityPanel)
        func tool(_ id: String, _ diffs: [(String, String)]) throws -> Entry {
            let content = diffs.map { #"{"type":"diff","path":"\#($0.0)","new":"\#($0.1)"}"# }.joined(separator: ",")
            let json = #"{"id":\#(id),"kind":{"type":"tool","call_id":"c\#(id)","title":"Edit","content":[\#(content)]}}"#
            return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
        }
        transcript.entries = [try tool("1", [("a.swift", "one"), ("b.swift", "b")]), try tool("2", [("a.swift", "two")])]
        #expect(transcript.changedFiles.map(\.path) == ["a.swift", "b.swift"])
        #expect(transcript.changedFiles.first?.new == "two")
        #expect(transcript.hasActivityPanel)
    }

    @Test func stopEncodesAsTheChatCommand() throws {
        let data = try JSONEncoder().encode(ChatCommand.stopActivity(callId: "toolu_1"))
        let json = try #require(JSONSerialization.jsonObject(with: data) as? [String: String])
        #expect(json == ["command": "stop_activity", "call_id": "toolu_1"])
        #expect(Expected(.stopActivity(callId: "toolu_1")) == nil)
    }
}
