import Foundation
import SheprdRemoteCore

/// A chat row with where it lives, for lists across computers.
struct ChatItem: Identifiable, Hashable {
    let computer: Computer
    let workspace: Home.Workspace
    let chat: Home.ChatRow
    /// This phone answered its question; the answer is on its way.
    var answered = false
    /// When the agent last answered: the computer's time, else when this
    /// phone saw it.
    var lastAnswer: Date?
    /// The composer holds something not sent yet.
    var drafted = false

    var id: String { "\(computer.device)/\(chat.pane)" }
    var key: PaneKey { PaneKey(device: computer.device, pane: chat.pane) }

    /// The question waiting, with its options.
    var question: Permission? { answered ? nil : chat.summary?.question }
    var needsYou: Bool { question != nil || (!answered && chat.status == .blocked) }
    var working: Bool { !needsYou && (chat.status == .working || chat.summary?.status == .working) }
    /// It replied since anyone last read it, on any device.
    var unread: Bool { chat.unread == true }
}

enum FleetError: Error, LocalizedError {
    case offline(String)
    /// A call to one of the demo's computers (`DemoFleet`).
    case demo

    var errorDescription: String? {
        switch self {
        case let .offline(name): "\(name) is not connected."
        case .demo: "This is a demo, so nothing is sent. Pair a computer to use it for real."
        }
    }
}

/// A terminal tab with where it lives.
struct TerminalItem: Identifiable, Hashable {
    let computer: Computer
    let workspace: Home.Workspace
    let terminal: Home.TerminalRow

    var id: String { "\(computer.device)/\(terminal.tab)" }

    /// The agent Herdr detects in it, such as `claude` run in a shell.
    var agent: Home.TerminalRow.DetectedAgent? { terminal.agent }
    var title: String { agent?.title ?? terminal.label }
    /// What closing its row closes: the whole tab.
    var closeTarget: CloseTarget? {
        terminal.firstPane.map { .tab(PaneKey(device: computer.device, pane: $0.pane), title: terminal.label) }
    }
    var needsYou: Bool { agent?.status == .blocked }
    /// When the computer saw its agent last change state.
    var changed: Date? { agent?.changedMs.map { Date(timeIntervalSince1970: Double($0) / 1000) } }
    var working: Bool { agent?.status == .working }
    /// It finished since anyone last looked at it, on any device.
    var unread: Bool { agent?.unread == true }

    /// Where a list opens it: an agent's conversation, when its computer
    /// shows it as a chat, else the agent's pane, else the focused one.
    var route: Route? {
        if let agent, agent.opensAsChat, let pane = terminal.agentPane {
            return .chat(PaneKey(device: computer.device, pane: pane.pane), title: title)
        }
        return terminalRoute
    }

    /// The terminal itself, at the agent's pane or the focused one.
    var terminalRoute: Route? {
        terminal.agentPane.map { .term(PaneKey(device: computer.device, pane: $0.pane), title: terminal.label) }
    }
}
