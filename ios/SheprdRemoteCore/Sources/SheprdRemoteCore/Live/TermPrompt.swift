import Foundation

/// The question a terminal agent waits on, as its computer read it from the
/// pane's screen (`GET /api/term/{pane}/prompt`): Claude Code's questions
/// and permission prompts, and Codex's approvals.
public struct TermPrompt: Codable, Hashable, Sendable {
    /// What it asks, top to bottom.
    public var question: [String]
    public var choices: [Choice]
    /// The highlighted choice's index in `choices`.
    public var selected: Int

    public struct Choice: Codable, Hashable, Sendable, Identifiable {
        public var number: UInt32
        public var label: String
        public var detail: String?
        /// It takes typed words.
        public var text: Bool?

        public var id: UInt32 { number }
        public var takesWords: Bool { text == true }
    }

    /// The answer's envelope: `prompt` is absent when nothing waits.
    public struct Answer: Codable, Sendable {
        public var prompt: TermPrompt?
    }
}
