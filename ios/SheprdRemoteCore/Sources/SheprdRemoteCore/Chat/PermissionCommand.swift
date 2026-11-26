import Foundation

/// The shell command a permission asks to run, read back from its title:
/// the agent bridge titles a command `` `first line` `` and adds
/// ` (+N more lines)` when the command has more (`command_title` in
/// `sheprd-acp`).
public struct PermissionCommand: Hashable, Sendable {
    /// The command's first line, without the backticks.
    public var text: String
    /// Lines of the command the title leaves out.
    public var moreLines: Int

    public init?(title: String) {
        guard title.first == "`" else { return nil }
        let body = title.dropFirst()
        guard let close = body.lastIndex(of: "`") else { return nil }
        let text = String(body[..<close])
        let rest = body[body.index(after: close)...]
        guard !text.isEmpty, let more = Self.moreLines(rest) else { return nil }
        self.text = text
        self.moreLines = more
    }

    /// Nothing, or ` (+1 more line)` / ` (+N more lines)`.
    private static func moreLines(_ rest: Substring) -> Int? {
        if rest.isEmpty { return 0 }
        guard rest.hasPrefix(" (+"), rest.hasSuffix(")") else { return nil }
        let inner = rest.dropFirst(3).dropLast()
        guard let space = inner.firstIndex(of: " "), let count = Int(inner[..<space]), count > 0 else { return nil }
        let word = inner[inner.index(after: space)...]
        return word == (count == 1 ? "more line" : "more lines") ? count : nil
    }
}

extension Permission {
    /// The command this permission asks to run, when its title is one.
    public var command: PermissionCommand? { PermissionCommand(title: title) }
}
