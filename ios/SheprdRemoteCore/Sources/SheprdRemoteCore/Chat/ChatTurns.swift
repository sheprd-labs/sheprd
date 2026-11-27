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
