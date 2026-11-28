import Foundation

/// A screen of the app an agent on a computer may open
/// (`control::phone::Screen` there). Panes and workspaces are that
/// computer's, by the ids its home lists.
public enum AgentScreen: Codable, Hashable, Sendable {
    case home
    case workspaces
    case pullRequests
    case settings
    case settingsPage(AgentSettingsPage)
    case newWorkspace
    case chat(pane: String)
    case term(pane: String)
    case workspace(String)
    case changes(workspace: String)

    enum CodingKeys: String, CodingKey { case screen, pane, workspace, page }

    /// A screen this app does not know, from a newer computer.
    public struct Unknown: Error, Hashable {
        public let name: String
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .screen) {
        case "home": self = .home
        case "workspaces": self = .workspaces
        case "pull_requests": self = .pullRequests
        case "settings": self = .settings
        case "settings_page":
            let name = try c.decode(String.self, forKey: .page)
            guard let page = AgentSettingsPage(rawValue: name) else { throw Unknown(name: name) }
            self = .settingsPage(page)
        case "new_workspace": self = .newWorkspace
        case "chat": self = .chat(pane: try c.decode(String.self, forKey: .pane))
        case "term": self = .term(pane: try c.decode(String.self, forKey: .pane))
        case "workspace": self = .workspace(try c.decode(String.self, forKey: .workspace))
        case "changes": self = .changes(workspace: try c.decode(String.self, forKey: .workspace))
        case let other: throw Unknown(name: other)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .home: try c.encode("home", forKey: .screen)
        case .workspaces: try c.encode("workspaces", forKey: .screen)
        case .pullRequests: try c.encode("pull_requests", forKey: .screen)
        case .settings: try c.encode("settings", forKey: .screen)
        case let .settingsPage(page):
            try c.encode("settings_page", forKey: .screen)
            try c.encode(page, forKey: .page)
        case .newWorkspace: try c.encode("new_workspace", forKey: .screen)
        case let .chat(pane):
            try c.encode("chat", forKey: .screen)
            try c.encode(pane, forKey: .pane)
        case let .term(pane):
            try c.encode("term", forKey: .screen)
            try c.encode(pane, forKey: .pane)
        case let .workspace(workspace):
            try c.encode("workspace", forKey: .screen)
            try c.encode(workspace, forKey: .workspace)
        case let .changes(workspace):
            try c.encode("changes", forKey: .screen)
            try c.encode(workspace, forKey: .workspace)
        }
    }
}

/// What an agent on a computer tells the app to do, in a `drive` message.
public enum AgentCommand: Hashable, Sendable {
    /// Say which screen is in front.
    case state
    case open(AgentScreen)
    /// Put a note in front of the person. `from` is the asking agent's
    /// chat or terminal, which a tap on the note opens.
    case show(text: String, from: AgentScreen? = nil)
    case guide(text: String, highlight: AgentHighlight?, from: AgentScreen? = nil)
    case highlight(AgentHighlight, from: AgentScreen? = nil)
    case clearGuide
    /// Lay `files` (paths in the screen's folder to their Luau) over the
    /// Luau screen `screen`'s bundled ones and reload it; none goes back
    /// to the bundled ones. A debug build only.
    case luau(screen: String, files: [String: String])
    /// A picture of what the app shows, uploaded for the agent.
    case snapshot
    /// A command, or a screen, this app does not know: refused as `other`.
    case unknown

    enum CodingKeys: String, CodingKey { case command, to, text, highlight, target, from, screen, files }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // A screen this app does not know, from a newer computer, leaves
        // the note opening nothing rather than refused.
        let from = (try? c.decodeIfPresent(AgentScreen.self, forKey: .from)) ?? nil
        switch try c.decodeIfPresent(String.self, forKey: .command) {
        case "state": self = .state
        case "open":
            do {
                self = .open(try c.decode(AgentScreen.self, forKey: .to))
            } catch is AgentScreen.Unknown {
                self = .unknown
            }
        case "show": self = .show(text: try c.decode(String.self, forKey: .text), from: from)
        case "clear_guide": self = .clearGuide
        case "luau":
            self = .luau(
                screen: try c.decode(String.self, forKey: .screen),
                files: try c.decodeIfPresent([String: String].self, forKey: .files) ?? [:]
            )
        case "snapshot": self = .snapshot
        case "highlight":
            if let target = AgentHighlight(rawValue: try c.decode(String.self, forKey: .target)) { self = .highlight(target, from: from) }
            else { self = .unknown }
        case "guide":
            let name = try c.decodeIfPresent(String.self, forKey: .highlight)
            let target = name.flatMap(AgentHighlight.init(rawValue:))
            if name != nil, target == nil { self = .unknown }
            else { self = .guide(text: try c.decode(String.self, forKey: .text), highlight: target, from: from) }
        default: self = .unknown
        }
    }
}

/// What the app shows, as it tells the computer.
public struct AgentPhoneState: Codable, Hashable, Sendable {
    /// The screen in front, when it is a tab or one of that computer's.
    public var screen: AgentScreen?
    /// The app is locked: it shows nothing and opens nothing.
    public var locked: Bool

    public init(screen: AgentScreen?, locked: Bool = false) {
        self.screen = screen
        self.locked = locked
    }

    enum CodingKeys: String, CodingKey { case screen, locked }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        screen = try c.decodeIfPresent(AgentScreen.self, forKey: .screen)
        locked = try c.decodeIfPresent(Bool.self, forKey: .locked) ?? false
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(screen, forKey: .screen)
        if locked { try c.encode(true, forKey: .locked) }
    }
}

/// Why the app did not do what an agent asked.
public enum AgentRefusal: String, Codable, Hashable, Sendable {
    case locked
    case notFound = "not_found"
    /// Agents are off for that computer.
    case off
    case other
}

/// What loading an agent's Luau found, each list short enough for the
/// answer to fit the live socket (`control::phone::LuauReport` there).
public struct AgentLuauReport: Codable, Hashable, Sendable {
    /// The screen is open and redrew; else its files were only checked.
    public var shown: Bool
    public var problems: [String]
    public var warnings: [String]
    public var logs: [String]

    /// The most lines of each kind, the longest line, and the most bytes
    /// of them all as JSON, escapes included: the computer closes a socket
    /// on a message over 4 KiB.
    public static let maxLines = 12
    public static let maxLength = 160
    public static let maxBytes = 3000

    /// Problems first, then warnings, then the latest logs, until
    /// `maxBytes` of them encoded.
    public init(shown: Bool, problems: [String], warnings: [String] = [], logs: [String] = []) {
        var left = Self.maxBytes
        let cut = { (lines: [String]) -> [String] in
            var kept: [String] = []
            for line in lines.prefix(Self.maxLines) {
                let short = String(line.prefix(Self.maxLength))
                // Its quotes and escapes, and the comma after it.
                let size = Self.encodedSize(short) + 1
                guard size <= left else { break }
                left -= size
                kept.append(short)
            }
            return kept
        }
        self.shown = shown
        self.problems = cut(problems)
        self.warnings = cut(warnings)
        self.logs = cut(Array(logs.suffix(Self.maxLines)))
    }

    /// How many bytes `line` takes as a JSON string, as `JSONEncoder`
    /// writes it: a control character takes six, `"`, `\` and `/` two.
    static func encodedSize(_ line: String) -> Int {
        line.utf8.reduce(2) { size, byte in
            switch byte {
            case 0x22, 0x5C, 0x2F, 0x08, 0x0C, 0x0A, 0x0D, 0x09: size + 2
            case 0..<0x20: size + 6
            default: size + 1
            }
        }
    }
}

/// What the app did with a `drive`, in its `driven` answer.
public enum AgentOutcome: Hashable, Sendable {
    case done(AgentPhoneState)
    case refused(AgentRefusal)
    case luau(AgentLuauReport)
    /// The id the computer gave the picture the app uploaded.
    case snapshot(upload: String)
}
