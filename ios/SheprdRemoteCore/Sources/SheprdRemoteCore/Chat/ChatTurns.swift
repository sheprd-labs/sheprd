import Foundation

/// Splitting a transcript into prompt turns, summarizing folded turns, and
/// tracking prompts-only filter state.
public enum ChatTurns {
    /// A transcript's entries split where each user prompt starts; whatever
    /// came before the first prompt is a turn of its own, without a heading.
    public static func split(_ entries: [Entry]) -> [ArraySlice<Entry>] {
        var turns: [ArraySlice<Entry>] = []
        var start = entries.startIndex
        for index in entries.indices where index > start {
            if case .user = entries[index].kind {
                turns.append(entries[start ..< index])
                start = index
            }
        }
        if start < entries.endIndex { turns.append(entries[start...]) }
        return turns
    }

    /// Number of user prompts sent in `entries`.
    public static func promptCount(in entries: [Entry]) -> Int {
        entries.count { if case .user = $0.kind { true } else { false } }
    }

    /// One-line summary of a turn's tool calls, matching the disclosure row.
    public static func stepsLine(_ tools: [Tool]) -> String {
        let edited = Set(tools.filter { $0.kind == .edit }.flatMap(\.locations)).count
        let steps = "\(tools.count) step\(tools.count == 1 ? "" : "s")"
        return edited > 0 ? "\(steps) · \(edited) file\(edited == 1 ? "" : "s") changed" : steps
    }

    /// One-line duration summary of how a turn ended.
    public static func turnLine(_ summary: TurnSummary) -> String {
        let seconds = Int(summary.durationMs / 1000)
        let time = seconds >= 60 ? "\(seconds / 60)m \(seconds % 60)s" : "\(seconds)s"
        return summary.end == .done ? "Done in \(time)" : "Ended (\(summary.end.rawValue)) after \(time)"
    }

    /// Concise metadata shown under a prompt while its turn is collapsed in
    /// prompts-only mode, or `nil` when nothing has answered yet.
    public static func collapsedSummary(for entries: ArraySlice<Entry>) -> String? {
        let tools = entries.compactMap { if case let .tool(tool) = $0.kind { tool } else { nil } }
        var turnSummary: TurnSummary?
        var hasBody = false
        for entry in entries.dropFirst() {
            switch entry.kind {
            case let .turn(summary):
                turnSummary = summary
            case .agent, .thought, .image, .notice:
                hasBody = true
            case .tool, .user:
                break
            }
        }

        var parts: [String] = []
        if !tools.isEmpty { parts.append(stepsLine(tools)) }
        if let turnSummary { parts.append(turnLine(turnSummary)) }
        if !parts.isEmpty { return parts.joined(separator: " · ") }
        return hasBody ? "Response" : nil
    }
}

/// Filter state for showing only the user's sent prompts, with per-turn
/// inline accordion peek while filtered.
public struct TurnFilterState: Equatable, Sendable {
    public var promptsOnly: Bool
    /// Prompt entry IDs temporarily expanded inline while `promptsOnly` is on.
    public var peeked: Set<EntryId>

    public init(promptsOnly: Bool = false, peeked: Set<EntryId> = []) {
        self.promptsOnly = promptsOnly
        self.peeked = peeked
    }

    /// Toggles prompts-only mode; leaving prompts-only clears inline peeks.
    public mutating func toggleFilter() {
        promptsOnly.toggle()
        if !promptsOnly { peeked.removeAll() }
    }

    /// Exits prompts-only mode and clears any inline peeks.
    public mutating func clear() {
        promptsOnly = false
        peeked.removeAll()
    }

    /// Expands or collapses a single turn inline while prompts-only is active.
    public mutating func togglePeek(_ id: EntryId) {
        guard promptsOnly else { return }
        if peeked.contains(id) {
            peeked.remove(id)
        } else {
            peeked.insert(id)
        }
    }

    /// Whether a turn is currently peeked inline during prompts-only mode.
    public func isPeeked(_ id: EntryId?) -> Bool {
        guard promptsOnly, let id else { return false }
        return peeked.contains(id)
    }

    /// Whether a turn's steps, response, and diffs should be visible.
    public func isBodyVisible(for id: EntryId?) -> Bool {
        !promptsOnly || isPeeked(id)
    }

    /// Turns to render and how many earlier turns stay folded behind
    /// "Show earlier". In prompts-only mode, all prompt-bearing turns show so
    /// the reader sees every prompt without tapping to load more.
    public func visibleTurns(
        from turns: [ArraySlice<Entry>],
        shownTurns: Int
    ) -> (turns: [ArraySlice<Entry>], hidden: Int) {
        if promptsOnly {
            let promptTurns = turns.filter { turn in
                if case .user = turn.first?.kind { true } else { false }
            }
            return (promptTurns, 0)
        }
        let hidden = max(turns.count - shownTurns, 0)
        return (Array(turns.dropFirst(hidden)), hidden)
    }

    /// Ensures `shownTurns` includes `id` when jumping from a peeked earlier
    /// turn back to the full conversation.
    public static func expandedShownTurns(
        current: Int,
        allTurns: [ArraySlice<Entry>],
        jumpingTo id: EntryId
    ) -> Int {
        guard let index = allTurns.firstIndex(where: { $0.first?.id == id }) else {
            return current
        }
        return max(current, allTurns.count - index)
    }
}

/// One flat row in a chat's `LazyVStack`. Flattening turns into direct
/// `LazyVStack` children lets SwiftUI materialize only the rows near the
/// viewport instead of building an entire long turn's entries, markdown
/// blocks, and diffs synchronously during a navigation push.
public struct ChatTurnRow: Identifiable, Sendable {
    public enum ID: Hashable, Sendable {
        case divider(EntryId)
        case prompt(EntryId)
        case steps(EntryId)
        case entry(EntryId)
        case markdownBlock(EntryId, Int)
        case diff(EntryId, String, Int)
        case showFullHere(EntryId)
    }

    public enum Kind: Sendable {
        case divider
        case prompt(turnId: EntryId, text: String, isLatest: Bool, collapsedMeta: String?)
        case steps(tools: [Tool])
        case entry(Entry)
        case agentMessage(id: EntryId, text: String, message: AgentMessage)
        case markdownBlock(MarkdownBlock)
        case diff(path: String, old: String?, new: String)
        case showFullHere(turnId: EntryId)
    }

    public let id: ID
    public let kind: Kind

    public init(id: ID, kind: Kind) {
        self.id = id
        self.kind = kind
    }
}

extension ChatTurns {
    /// Flattens visible turns into direct `LazyVStack` rows with stable,
    /// unique IDs. Plain agent replies (without bubble or screen effects) are
    /// flattened to individual `MarkdownBlock` rows so even a single long
    /// reply stays lazy; replies with effects carry their pre-parsed
    /// `AgentMessage` so `AgentMessageView` never parses them again.
    public static func rows(
        from turns: [ArraySlice<Entry>],
        latestTurnId: EntryId?,
        filter: TurnFilterState,
        parse: (EntryId, String) -> AgentMessage = { _, text in AgentMessage.parse(text) }
    ) -> [ChatTurnRow] {
        var rows: [ChatTurnRow] = []
        for (offset, turn) in turns.enumerated() {
            guard let turnId = turn.first?.id else { continue }
            if offset > 0 {
                rows.append(ChatTurnRow(id: .divider(turnId), kind: .divider))
            }
            let prompt: (text: String, rest: ArraySlice<Entry>)? = if case let .user(text, _) = turn.first?.kind {
                (text, turn.dropFirst())
            } else {
                nil
            }
            let showBody = filter.isBodyVisible(for: turnId)
            if let prompt {
                let collapsedMeta = filter.promptsOnly && !showBody ? collapsedSummary(for: turn) : nil
                rows.append(ChatTurnRow(
                    id: .prompt(turnId),
                    kind: .prompt(
                        turnId: turnId,
                        text: prompt.text,
                        isLatest: turnId == latestTurnId,
                        collapsedMeta: collapsedMeta
                    )
                ))
            }
            guard showBody else { continue }
            let tools = turn.compactMap { if case let .tool(tool) = $0.kind { tool } else { nil } }
            if !tools.isEmpty {
                rows.append(ChatTurnRow(id: .steps(turnId), kind: .steps(tools: tools)))
            }
            for entry in prompt?.rest ?? turn {
                switch entry.kind {
                case .tool:
                    continue
                case let .agent(text):
                    let message = parse(entry.id, text)
                    let plainMarkdown = message.screen == nil && message.parts.allSatisfy {
                        if case .markdown = $0 { true } else { false }
                    }
                    if plainMarkdown {
                        var blockIndex = 0
                        for part in message.parts {
                            if case let .markdown(blocks) = part {
                                for block in blocks {
                                    rows.append(ChatTurnRow(
                                        id: .markdownBlock(entry.id, blockIndex),
                                        kind: .markdownBlock(block)
                                    ))
                                    blockIndex += 1
                                }
                            }
                        }
                    } else {
                        rows.append(ChatTurnRow(
                            id: .entry(entry.id),
                            kind: .agentMessage(id: entry.id, text: text, message: message)
                        ))
                    }
                default:
                    rows.append(ChatTurnRow(id: .entry(entry.id), kind: .entry(entry)))
                }
            }
            for tool in tools {
                var diffIndex = 0
                for item in tool.content {
                    if case let .diff(path, old, new) = item {
                        rows.append(ChatTurnRow(
                            id: .diff(turnId, tool.callId, diffIndex),
                            kind: .diff(path: path, old: old, new: new)
                        ))
                        diffIndex += 1
                    }
                }
            }
            if filter.promptsOnly, filter.isPeeked(turnId) {
                rows.append(ChatTurnRow(id: .showFullHere(turnId), kind: .showFullHere(turnId: turnId)))
            }
        }
        return rows
    }
}

/// Caches split turns, flattened `LazyVStack` rows, and parsed `AgentMessage`s
/// per open chat so scroll-state updates, filter toggles, and streaming edits
/// never re-parse unchanged replies.
public final class ChatRowCache {
    public struct Snapshot {
        public let turns: [ArraySlice<Entry>]
        public let promptCount: Int
        public let hidden: Int
        public let latestTurnId: EntryId?
        public let rows: [ChatTurnRow]
    }

    private struct CachedMessage {
        let text: String
        let message: AgentMessage
    }

    private let parseMessage: (String) -> AgentMessage
    private var parsed: [EntryId: CachedMessage] = [:]
    private var lastEntries: [Entry]?
    private var lastShownTurns = -1
    private var lastFilter = TurnFilterState()
    private var lastSnapshot: Snapshot?

    public init(parse: @escaping (String) -> AgentMessage = AgentMessage.parse) {
        self.parseMessage = parse
    }

    /// Returns the flattened view data for `entries`, `shownTurns`, and `filter`,
    /// reusing the last snapshot when inputs are unchanged and reusing cached
    /// `AgentMessage` parses for every unchanged agent reply.
    public func snapshot(
        entries: [Entry],
        shownTurns: Int,
        filter: TurnFilterState
    ) -> Snapshot {
        if let lastSnapshot,
           lastShownTurns == shownTurns,
           lastFilter == filter,
           lastEntries == entries {
            return lastSnapshot
        }
        let turns = ChatTurns.split(entries)
        let promptCount = ChatTurns.promptCount(in: entries)
        let visible = filter.visibleTurns(from: turns, shownTurns: shownTurns)
        let latestTurnId = turns.last?.first?.id
        var seen: Set<EntryId> = []
        let rows = ChatTurns.rows(from: visible.turns, latestTurnId: latestTurnId, filter: filter) { id, text in
            seen.insert(id)
            if let cached = parsed[id], cached.text == text {
                return cached.message
            }
            let message = parseMessage(text)
            parsed[id] = CachedMessage(text: text, message: message)
            return message
        }
        // Keep the parsed cache bounded to entries still present in the transcript.
        if lastEntries != entries {
            let liveIds = Set(entries.compactMap { entry -> EntryId? in
                if case .agent = entry.kind { entry.id } else { nil }
            })
            parsed = parsed.filter { liveIds.contains($0.key) || seen.contains($0.key) }
        }
        let snapshot = Snapshot(
            turns: turns,
            promptCount: promptCount,
            hidden: visible.hidden,
            latestTurnId: latestTurnId,
            rows: rows
        )
        lastEntries = entries
        lastShownTurns = shownTurns
        lastFilter = filter
        lastSnapshot = snapshot
        return snapshot
    }
}
