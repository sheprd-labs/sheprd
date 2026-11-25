/// The logo Sheprd and the web app draw for an agent
/// (`assets/icons/agent-*.svg`), by the name Herdr gives it.
public enum AgentMark {
    /// Matched in order: "copilot" holds "pi", so Copilot comes first.
    public static let all = ["claude", "codex", "gemini", "copilot", "opencode", "amp", "grok", "cursor", "agy", "muse", "pi", "generic"]

    /// `agent`'s logo, or `generic` for one without its own.
    public static func named(_ agent: String?) -> String {
        let name = (agent ?? "").lowercased()
        if name.contains("antigravity") { return "agy" }
        return all.first { name.contains($0) } ?? "generic"
    }
}
