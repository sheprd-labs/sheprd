import SheprdRemoteCore
import SwiftUI

extension View {
    /// Opens a tapped `ScreenLink` on the computer `device`, the one whose
    /// chat shows it, and leaves every other link to the handler further
    /// out, so put it inside `browsesLinks`.
    func opensScreenLinks(on device: String) -> some View {
        modifier(ScreenLinkOpening(device: device))
    }
}

/// Pushes a route onto the stack of a window that shows one chat,
/// terminal, workspace, or changes of its own (iPad).
struct WindowRoutes: Sendable {
    let push: @MainActor @Sendable (Route) -> Void
}

extension EnvironmentValues {
    /// Set by a window of its own; `nil` in the app's main window, whose
    /// root view reads the pending routes.
    @Entry var windowRoutes: WindowRoutes?
}

private struct ScreenLinkOpening: ViewModifier {
    let device: String
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var outer
    @Environment(\.windowRoutes) private var windowRoutes

    func body(content: Content) -> some View {
        content.environment(\.openURL, OpenURLAction { url in
            guard let screen = ScreenLink.screen(url) else {
                outer(url)
                return .handled
            }
            // The person's own tap: no banner, and no need for "Let agents
            // open screens", which is about agents acting on their own.
            let opened: Bool
            if let windowRoutes {
                let route = AgentControl.route(for: screen, on: device, fleet: model.fleet)
                if let route { windowRoutes.push(route) }
                opened = route != nil
            } else {
                opened = model.agents.open(screen, on: device, model: model)
            }
            if !opened { model.message = "That is no longer on \(computerName)." }
            return .handled
        })
    }

    private var computerName: String {
        model.fleet.machines.first { $0.id == device }?.computer.name ?? "your computer"
    }
}
