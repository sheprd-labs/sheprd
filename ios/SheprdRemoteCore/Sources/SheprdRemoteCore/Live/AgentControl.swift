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
    /// Put a note in front of the person.
    case show(text: String)
    case guide(text: String, highlight: AgentHighlight?)
    case highlight(AgentHighlight)
    case clearGuide
    /// A command, or a screen, this app does not know: refused as `other`.
    case unknown

    enum CodingKeys: String, CodingKey { case command, to, text, highlight, target }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decodeIfPresent(String.self, forKey: .command) {
        case "state": self = .state
        case "open":
            do {
                self = .open(try c.decode(AgentScreen.self, forKey: .to))
            } catch is AgentScreen.Unknown {
                self = .unknown
            }
        case "show": self = .show(text: try c.decode(String.self, forKey: .text))
        case "clear_guide": self = .clearGuide
        case "highlight":
            if let target = AgentHighlight(rawValue: try c.decode(String.self, forKey: .target)) { self = .highlight(target) }
            else { self = .unknown }
        case "guide":
            let name = try c.decodeIfPresent(String.self, forKey: .highlight)
            let target = name.flatMap(AgentHighlight.init(rawValue:))
            if name != nil, target == nil { self = .unknown }
            else { self = .guide(text: try c.decode(String.self, forKey: .text), highlight: target) }
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

/// What the app did with a `drive`, in its `driven` answer.
public enum AgentOutcome: Hashable, Sendable {
    case done(AgentPhoneState)
    case refused(AgentRefusal)
}
