import Foundation

/// A link an agent writes in its chat to a screen of its own computer, so
/// the person opens what it just made with a tap: `sheprd://chat/PANE` (an
/// agent's chat, else its terminal), `sheprd://term/PANE`,
/// `sheprd://workspace/WORKSPACE`, or `sheprd://changes/WORKSPACE`, with
/// Herdr's ids. It names no computer: the chat it is in opens it on that
/// chat's, so text from one computer can never reach another's screens.
/// It only shows a screen, as `sheprd phone open` does, and never acts.
public enum ScreenLink {
    public static let scheme = "sheprd"

    /// The screen `url` names; `nil` for any other URL or kind.
    public static func screen(_ url: URL) -> AgentScreen? {
        guard url.scheme?.lowercased() == scheme,
              let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let kind = components.host?.lowercased()
        else { return nil }
        // One path segment: the id, as written (Herdr's ids keep their case).
        let segments = components.path.split(separator: "/", omittingEmptySubsequences: true)
        guard segments.count == 1, let id = String(segments[0]).removingPercentEncoding, !id.isEmpty
        else { return nil }
        switch kind {
        case "chat": return .chat(pane: id)
        case "term": return .term(pane: id)
        case "workspace": return .workspace(id)
        case "changes": return .changes(workspace: id)
        default: return nil
        }
    }

    /// Whether `listed`, a pane id as the phone lists it, is the pane
    /// `named` that an agent wrote: the same id, or, for a herdr-core
    /// terminal the desktop and `sheprd chat list` call `core:ADDRESS`, the
    /// phone's `core:TAG-HEX`, its boot's tag then the address's bytes in
    /// hex (`control::favorites::core_pane`).
    public static func pane(_ listed: String, isNamed named: String) -> Bool {
        if listed == named { return true }
        let prefix = "core:"
        guard named.hasPrefix(prefix), listed.hasPrefix(prefix) else { return false }
        let address = named.dropFirst(prefix.count)
        guard !address.isEmpty, !address.contains("-") else { return false }
        let hex = address.utf8.map { String(format: "%02x", $0) }.joined()
        let rest = listed.dropFirst(prefix.count)
        guard let dash = rest.firstIndex(of: "-") else { return false }
        let tag = rest[..<dash]
        return tag.count == 8 && tag.allSatisfy(\.isHexDigit) && rest[rest.index(after: dash)...] == hex[...]
    }
}
