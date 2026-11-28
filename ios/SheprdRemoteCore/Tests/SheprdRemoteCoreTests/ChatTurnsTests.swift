import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite("ChatTurns")
struct ChatTurnsTests {
    private func makeTool(id: String, kind: ToolKind, locations: [String] = []) throws -> Tool {
        let locJSON = locations.map { "\"\($0)\"" }.joined(separator: ",")
        let json = """
        {"call_id":"\(id)","title":"\(id)","kind":"\(kind.rawValue)","status":"completed","locations":[\(locJSON)]}
        """
        return try JSONDecoder().decode(Tool.self, from: Data(json.utf8))
    }

    private func makeTurnSummary(durationMs: UInt64, end: String = "done") throws -> TurnSummary {
        let json = """
        {"duration_ms":\(durationMs),"end":"\(end)"}
        """
        return try JSONDecoder().decode(TurnSummary.self, from: Data(json.utf8))
    }

    @Test func splitsEntriesAtEachUserPrompt() {
        let entries: [Entry] = [
            Entry(id: 1, kind: .agent(text: "Welcome")),
            Entry(id: 2, kind: .user(text: "First prompt", files: 0)),
            Entry(id: 3, kind: .agent(text: "First answer")),
            Entry(id: 4, kind: .user(text: "Second prompt", files: 0)),
            Entry(id: 5, kind: .agent(text: "Second answer")),
        ]
        let turns = ChatTurns.split(entries)
        #expect(turns.count == 3)
        #expect(turns[0].map(\.id) == [1])
        #expect(turns[1].map(\.id) == [2, 3])
        #expect(turns[2].map(\.id) == [4, 5])
        #expect(ChatTurns.promptCount(in: entries) == 2)
    }

    @Test func collapsedSummaryCombinesStepsAndDuration() throws {
        let edit1 = try makeTool(id: "t1", kind: .edit, locations: ["a.swift", "b.swift"])
        let read1 = try makeTool(id: "t2", kind: .read, locations: ["c.swift"])
        let done = try makeTurnSummary(durationMs: 258_000)

        let turn: ArraySlice<Entry> = [
            Entry(id: 1, kind: .user(text: "Fix relay", files: 0)),
            Entry(id: 2, kind: .tool(edit1)),
            Entry(id: 3, kind: .tool(read1)),
            Entry(id: 4, kind: .agent(text: "Done")),
            Entry(id: 5, kind: .turn(done)),
        ][...]

        #expect(ChatTurns.collapsedSummary(for: turn) == "2 steps · 2 files changed · Done in 4m 18s")
    }

    @Test func collapsedSummaryHandlesReplyWithoutToolsOrSummary() {
        let replied: ArraySlice<Entry> = [
            Entry(id: 1, kind: .user(text: "Hi", files: 0)),
            Entry(id: 2, kind: .agent(text: "Hello!")),
        ][...]
        #expect(ChatTurns.collapsedSummary(for: replied) == "Response")

        let unanswered: ArraySlice<Entry> = [
            Entry(id: 1, kind: .user(text: "Hi", files: 0)),
        ][...]
        #expect(ChatTurns.collapsedSummary(for: unanswered) == nil)
    }

    @Test func filterShowsAllPromptTurnsAndSupportsInlinePeek() {
        let entries: [Entry] = [
            Entry(id: 1, kind: .agent(text: "Banner")),
            Entry(id: 2, kind: .user(text: "P1", files: 0)),
            Entry(id: 3, kind: .agent(text: "A1")),
            Entry(id: 4, kind: .user(text: "P2", files: 0)),
            Entry(id: 5, kind: .agent(text: "A2")),
            Entry(id: 6, kind: .user(text: "P3", files: 0)),
            Entry(id: 7, kind: .agent(text: "A3")),
        ]
        let turns = ChatTurns.split(entries)
        var filter = TurnFilterState()

        // Normal mode respects shownTurns=2
        let normal = filter.visibleTurns(from: turns, shownTurns: 2)
        #expect(normal.hidden == 2)
        #expect(normal.turns.map(\.first?.id) == [4, 6])
        #expect(filter.isBodyVisible(for: 4))

        // Toggling peek while unfiltered does nothing
        filter.togglePeek(4)
        #expect(filter.peeked.isEmpty)

        // Prompts-only mode shows all user-prompt turns and hides leading non-user turns
        filter.toggleFilter()
        let filtered = filter.visibleTurns(from: turns, shownTurns: 2)
        #expect(filtered.hidden == 0)
        #expect(filtered.turns.map(\.first?.id) == [2, 4, 6])
        #expect(!filter.isBodyVisible(for: 2))
        #expect(!filter.isBodyVisible(for: 4))

        // Peeking turn 2 expands only turn 2
        filter.togglePeek(2)
        #expect(filter.isPeeked(2))
        #expect(filter.isBodyVisible(for: 2))
        #expect(!filter.isBodyVisible(for: 4))

        // Jumping to turn 2 widens shownTurns so it stays visible in full mode
        let widened = TurnFilterState.expandedShownTurns(current: 2, allTurns: turns, jumpingTo: 2)
        #expect(widened == 3)

        // Clearing filter exits prompts-only and clears peeks
        filter.clear()
        #expect(!filter.promptsOnly)
        #expect(filter.peeked.isEmpty)
        #expect(filter.isBodyVisible(for: 4))
    }

    @Test func rowsFlattenPlainMarkdownBlocksAndKeepUniqueDiffIDs() throws {
        let edit1JSON = """
        {"call_id":"t1","title":"Edit A","kind":"edit","status":"completed","locations":["a.swift"],"content":[{"type":"diff","path":"a.swift","old":"1","new":"2"},{"type":"diff","path":"a.swift","old":"2","new":"3"}]}
        """
        let edit2JSON = """
        {"call_id":"t2","title":"Edit A again","kind":"edit","status":"completed","locations":["a.swift"],"content":[{"type":"diff","path":"a.swift","old":"3","new":"4"}]}
        """
        let edit1 = try JSONDecoder().decode(Tool.self, from: Data(edit1JSON.utf8))
        let edit2 = try JSONDecoder().decode(Tool.self, from: Data(edit2JSON.utf8))
        let entries: [Entry] = [
            Entry(id: 10, kind: .user(text: "Refactor", files: 0)),
            Entry(id: 11, kind: .tool(edit1)),
            Entry(id: 12, kind: .tool(edit2)),
            Entry(id: 13, kind: .agent(text: "First paragraph.\n\n- item one\n- item two\n\n```swift\nlet x = 1\n```")),
            Entry(id: 14, kind: .agent(text: ":::slam\nShipped!\n:::")),
            Entry(id: 15, kind: .agent(text: "::confetti\nAll green")),
        ]
        let turns = ChatTurns.split(entries)
        let rows = ChatTurns.rows(from: turns, latestTurnId: 10, filter: TurnFilterState())

        // All row IDs must be strictly unique even with repeated edits to the same file path.
        #expect(Set(rows.map(\.id)).count == rows.count)

        let ids = rows.map(\.id)
        #expect(ids == [
            .prompt(10),
            .steps(10),
            .markdownBlock(13, 0),
            .markdownBlock(13, 1),
            .markdownBlock(13, 2),
            .entry(14),
            .entry(15),
            .diff(10, "t1", 0),
            .diff(10, "t1", 1),
            .diff(10, "t2", 0),
        ])
    }

    @Test func rowsCollapseUnpeekedTurnsInPromptsOnlyMode() {
        let entries: [Entry] = [
            Entry(id: 1, kind: .user(text: "One", files: 0)),
            Entry(id: 2, kind: .agent(text: "Para A\n\nPara B")),
            Entry(id: 3, kind: .user(text: "Two", files: 0)),
            Entry(id: 4, kind: .agent(text: "Para C")),
        ]
        let turns = ChatTurns.split(entries)
        var filter = TurnFilterState(promptsOnly: true)

        let collapsed = ChatTurns.rows(from: turns, latestTurnId: 3, filter: filter)
        #expect(collapsed.map(\.id) == [.prompt(1), .divider(3), .prompt(3)])

        filter.togglePeek(1)
        let peeked = ChatTurns.rows(from: turns, latestTurnId: 3, filter: filter)
        #expect(peeked.map(\.id) == [
            .prompt(1),
            .markdownBlock(2, 0),
            .markdownBlock(2, 1),
            .showFullHere(1),
            .divider(3),
            .prompt(3),
        ])
    }

    @Test func rowCacheReusesParsedRepliesAndPreParsedEffects() {
        var parseCount = 0
        let cache = ChatRowCache { text in
            parseCount += 1
            return AgentMessage.parse(text)
        }
        var entries: [Entry] = [
            Entry(id: 1, kind: .user(text: "First", files: 0)),
            Entry(id: 2, kind: .agent(text: "First answer")),
            Entry(id: 3, kind: .user(text: "Second", files: 0)),
            Entry(id: 4, kind: .agent(text: ":::slam\nDone!\n:::")),
        ]

        // Initial render parses both agent replies once.
        let initial = cache.snapshot(entries: entries, shownTurns: 5, filter: TurnFilterState())
        #expect(parseCount == 2)
        if case let .agentMessage(id, _, parsed) = initial.rows.last?.kind {
            #expect(id == 4)
            #expect(parsed.bubbleEffects == [.slam])
        } else {
            Issue.record("expected pre-parsed .agentMessage row for effect-bearing reply")
        }

        // Scroll/UI body re-evaluation with identical inputs parses nothing.
        _ = cache.snapshot(entries: entries, shownTurns: 5, filter: TurnFilterState())
        #expect(parseCount == 2)

        // Filter toggle and peek reuse cached parses without re-parsing.
        var filter = TurnFilterState(promptsOnly: true)
        _ = cache.snapshot(entries: entries, shownTurns: 5, filter: filter)
        filter.togglePeek(1)
        _ = cache.snapshot(entries: entries, shownTurns: 5, filter: filter)
        #expect(parseCount == 2)

        // Streaming update to the last reply re-parses only the changed entry.
        entries[3] = Entry(id: 4, kind: .agent(text: ":::slam\nDone and verified!\n:::"))
        _ = cache.snapshot(entries: entries, shownTurns: 5, filter: TurnFilterState())
        #expect(parseCount == 3)
    }
}
