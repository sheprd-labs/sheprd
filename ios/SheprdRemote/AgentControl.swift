import Foundation
import os
import SheprdRemoteCore

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "agents")

/// Agents on a paired computer driving this app (`sheprd phone` there):
/// opening a screen, saying which one is in front, and showing a note. On
/// for every paired computer until its person turns it off in that
/// computer's page; refused while the app is locked. Every open and note
/// says which computer's agent did it, so the screen never changes
/// unexplained, and a tap on a note opens the agent's own chat.
@MainActor
@Observable
final class AgentControl {
    /// The computers whose person turned agents off, by fingerprint; every
    /// other paired computer's agents may drive the app.
    private(set) var turnedOff: Set<String> = Set(UserDefaults.standard.stringArray(forKey: AgentControl.key) ?? []) {
        didSet { UserDefaults.standard.set(turnedOff.sorted(), forKey: Self.key) }
    }
    /// Not the older `agentControl` key, which listed the computers turned
    /// on while agents were off by default.
    private static let key = "agentControlOff"
    /// The note on screen, from an agent or about what one opened. It
    /// shows in a window of its own above every sheet (`AgentBannerWindows`).
    var banner: Banner? {
        didSet { if banner != nil, let model = AppModel.current { AgentBannerWindows.install(model: model) } }
    }
    var guideHeight: CGFloat = 80
    var settingsPath: [AgentSettingsPage] = []
    var highlight: AgentHighlight?
    var guide: Banner? {
        didSet { if guide != nil, let model = AppModel.current { AgentBannerWindows.install(model: model) } }
    }
    private var guidingDevice: String?

    func clearGuide() {
        guide = nil
        highlight = nil
        guidingDevice = nil
    }

    /// What the window shows, as the root view reports it.
    @ObservationIgnored var front = Front()

    /// How long an open waits for the screen to follow before saying
    /// what is in front.
    static let settle: Duration = .milliseconds(350)

    struct Banner: Identifiable, Equatable {
        let id = UUID()
        let computer: String
        let text: String
        /// An agent's own note, rather than word of what it opened.
        let note: Bool
        /// The asking agent's chat or terminal on `device`, which a tap
        /// opens.
        var from: AgentScreen? = nil
        var device: String? = nil

        /// How long it stays unless tapped away.
        var shown: Duration { note ? .seconds(8) : .seconds(3) }
    }

    /// What the window shows: its tab, and the chat, terminal, or
    /// workspace on top of it.
    struct Front: Equatable {
        var tab = RootTab.home
        var route: Route?
        /// Something no `AgentScreen` names is on top, such as a page of
        /// the Workspaces tab's stack.
        var unnamed = false
        var settingsPage: AgentSettingsPage?
    }

    func allows(_ computer: Computer) -> Bool { !turnedOff.contains(computer.fingerprint.hex) }

    /// Turns agents on or off for `computer`, which hears it at once.
    func set(_ on: Bool, for computer: Computer, fleet: Fleet) {
        if on { turnedOff.remove(computer.fingerprint.hex) } else { turnedOff.insert(computer.fingerprint.hex) }
        fleet.setAgentControl(on, device: computer.device)
        if !on, guidingDevice == computer.device { clearGuide() }
    }

    /// Does what `computer`'s agent asked and answers it.
    func perform(_ id: UInt64, _ command: AgentCommand, from computer: Computer, model: AppModel) {
        let answer = { (outcome: AgentOutcome) in model.fleet.driven(id, outcome, device: computer.device) }
        guard allows(computer) else { return answer(.refused(.off)) }
        let locked = LockState.shared.hidesNames
        switch command {
        case .state:
            answer(.done(state(for: computer, model: model)))
        case .unknown:
            answer(.refused(.other))
        case _ where locked:
            answer(.refused(.locked))
        case .clearGuide:
            guard guidingDevice == nil || guidingDevice == computer.device else { return answer(.refused(.other)) }
            clearGuide()
            answer(.done(state(for: computer, model: model)))
        case let .guide(text, target, from):
            guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, text.unicodeScalars.count <= 500 else { return answer(.refused(.other)) }
            if target == .haptics, !FeedbackSettings.canTap { return answer(.refused(.notFound)) }
            if guidingDevice != computer.device { clearGuide() }
            guidingDevice = computer.device
            guide = Banner(computer: model.displayName(computer), text: text, note: true, from: from, device: computer.device)
            if let target { showHighlight(target, model: model) }
            answerAfterNavigation(computer, model: model, answer: answer)
        case let .highlight(target, from):
            if target == .haptics, !FeedbackSettings.canTap { return answer(.refused(.notFound)) }
            if guidingDevice != computer.device { clearGuide() }
            guidingDevice = computer.device
            guide = Banner(
                computer: model.displayName(computer), text: guide?.text ?? "Highlighted in Settings", note: true,
                from: from, device: computer.device
            )
            showHighlight(target, model: model)
            answerAfterNavigation(computer, model: model, answer: answer)
        case let .show(text, from):
            banner = Banner(computer: model.displayName(computer), text: text, note: true, from: from, device: computer.device)
            answer(.done(state(for: computer, model: model)))
        case let .open(screen):
            guard open(screen, on: computer.device, model: model) else { return answer(.refused(.notFound)) }
            log.notice("an agent on \(computer.name, privacy: .public) opened \(String(describing: screen), privacy: .public)")
            banner = Banner(computer: model.displayName(computer), text: "Opened by an agent on \(model.displayName(computer))", note: false)
            answerAfterNavigation(computer, model: model, answer: answer)
        }
    }

    /// A tap on `banner`: dismisses it and opens the chat or terminal of
    /// the agent that showed it, when it named one this app still lists.
    func tapped(_ banner: Banner, model: AppModel) {
        self.banner = nil
        clearGuide()
        guard let from = banner.from, let device = banner.device,
              let computer = model.computers.first(where: { $0.device == device }), allows(computer),
              !LockState.shared.hidesNames
        else { return }
        // Its chat may have closed since: the note just goes then.
        guard Self.route(for: from, on: computer.device, fleet: model.fleet) != nil else { return }
        // The person asked for the chat: a sheet over every tab, such as
        // the new workspace form or the voice sheet, would hide it.
        model.creating = nil
        model.voice = nil
        model.pendingPullRequest = nil
        _ = open(from, on: computer.device, model: model)
    }

    private func answerAfterNavigation(_ computer: Computer, model: AppModel, answer: @escaping (AgentOutcome) -> Void) {
        Task {
            try? await Task.sleep(for: Self.settle)
            answer(.done(self.state(for: computer, model: model)))
        }
    }

    /// Shows `screen` of the computer `device`; false when it lists no
    /// such pane or workspace. An agent's `sheprd phone open` and a
    /// `ScreenLink` tapped in its chat both come here.
    func open(_ screen: AgentScreen, on device: String, model: AppModel) -> Bool {
        switch screen {
        case .home: showTab(.home, model: model)
        case .workspaces: showTab(.workspaces, model: model)
        case .pullRequests: showTab(.pullRequests, model: model)
        case .settings:
            settingsPath = []
            showTab(.settings, model: model)
        case let .settingsPage(page):
            settingsPath = page.path
            showTab(.settings, model: model)
        case .newWorkspace: model.creating = .workspace(device: device)
        case .chat, .term, .changes:
            // A terminal agent this phone does not show as a chat opens
            // in its terminal.
            guard let route = Self.route(for: screen, on: device, fleet: model.fleet) else { return false }
            model.pendingRoute = route
        case let .workspace(workspace):
            guard Self.lists(workspace, on: device, fleet: model.fleet) else { return false }
            model.pendingTab = .home
            model.pendingWorkspace = WorkspaceKey(device: device, workspace: workspace)
        }
        return true
    }

    private func showHighlight(_ target: AgentHighlight, model: AppModel) {
        highlight = target
        settingsPath = target.page.path
        showTab(.settings, model: model)
    }

    /// The chat, terminal, workspace, or changes `screen` names on
    /// `device`, as a stack shows it; `nil` for a tab or form, or when the
    /// computer lists no such pane or workspace.
    static func route(for screen: AgentScreen, on device: String, fleet: Fleet) -> Route? {
        switch screen {
        case let .chat(pane): chat(pane, on: device, fleet: fleet) ?? term(pane, on: device, fleet: fleet)
        case let .term(pane): term(pane, on: device, fleet: fleet)
        case let .workspace(workspace):
            lists(workspace, on: device, fleet: fleet) ? .workspace(WorkspaceKey(device: device, workspace: workspace)) : nil
        case let .changes(workspace):
            lists(workspace, on: device, fleet: fleet) ? .changes(WorkspaceKey(device: device, workspace: workspace)) : nil
        case .home, .workspaces, .pullRequests, .settings, .settingsPage, .newWorkspace: nil
        }
    }

    /// A chat's route, or a terminal agent's conversation when its
    /// computer shows it as a chat, as a link to it opens (`AppModel.open`).
    /// `pane` may be a herdr-core terminal as the desktop names it
    /// (`ScreenLink.pane(_:isNamed:)`).
    private static func chat(_ pane: String, on device: String, fleet: Fleet) -> Route? {
        if let item = fleet.chatItems.first(where: { $0.computer.device == device && ScreenLink.pane($0.chat.pane, isNamed: pane) }) {
            return .chat(PaneKey(device: device, pane: item.chat.pane), title: item.chat.title)
        }
        let terminal = fleet.terminalItems.first { item in
            item.computer.device == device && item.agent?.pane.map { ScreenLink.pane($0, isNamed: pane) } == true
        }
        if case let .chat(key, title)? = terminal?.route { return .chat(key, title: title) }
        return nil
    }

    /// The route of the terminal holding `pane`.
    private static func term(_ pane: String, on device: String, fleet: Fleet) -> Route? {
        for item in fleet.terminalItems where item.computer.device == device {
            if let listed = item.terminal.panes.first(where: { ScreenLink.pane($0.pane, isNamed: pane) }) {
                return .term(PaneKey(device: device, pane: listed.pane), title: item.terminal.label)
            }
        }
        return nil
    }

    /// Shows `tab` at its root: an agent asking for a tab means the tab,
    /// not the page last open in it.
    private func showTab(_ tab: RootTab, model: AppModel) {
        model.pendingTabRoot = true
        model.pendingTab = tab
    }

    private static func lists(_ workspace: String, on device: String, fleet: Fleet) -> Bool {
        fleet.machines.first { $0.id == device }?.home?.workspaces.contains { $0.id == workspace } == true
    }

    /// What is in front, as `computer` names it: nothing of another
    /// computer's.
    func state(for computer: Computer, model: AppModel) -> AgentPhoneState {
        let locked = LockState.shared.hidesNames
        // Nothing shows behind the lock, so nothing is said of it.
        guard !locked else { return AgentPhoneState(screen: nil, locked: true) }
        return AgentPhoneState(screen: Self.screen(front, creating: model.creating, voice: model.voice != nil, device: computer.device))
    }

    static func screen(_ front: Front, creating: Creating?, voice: Bool, device: String) -> AgentScreen? {
        if let creating {
            if case let .workspace(on, _, _) = creating, on == nil || on == device { return .newWorkspace }
            return nil
        }
        if voice || front.unnamed { return nil }
        switch front.route {
        case let .chat(key, _)?: return key.device == device ? .chat(pane: key.pane) : nil
        case let .term(key, _)?: return key.device == device ? .term(pane: key.pane) : nil
        case let .workspace(key)?: return key.device == device ? .workspace(key.workspace) : nil
        case let .changes(key)?: return key.device == device ? .changes(workspace: key.workspace) : nil
        case nil: break
        }
        return switch front.tab {
        case .home: .home
        case .workspaces: .workspaces
        case .pullRequests: .pullRequests
        case .settings: front.settingsPage.map(AgentScreen.settingsPage) ?? .settings
        }
    }
}
