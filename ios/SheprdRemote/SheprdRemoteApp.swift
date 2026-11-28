import SwiftUI
import UserNotifications

/// Sheprd: the chats and terminals running in Sheprd on your
/// computers, with Live Activities on the lock screen. Each paired
/// computer keeps one live socket while the app is in front; launch draws
/// the cached state at once.
@main
struct SheprdRemoteApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    private var model: AppModel { delegate.model }
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .modifier(AppLock())
                .modifier(Appearance())
                .browsesLinks(othersToSystem: true)
                .onOpenURL { model.open($0) }
                .onChange(of: model.fleet.waiting, initial: true) { _, _ in
                    // While the app is in front, the badge follows the live
                    // counts; pushes set it otherwise, from the same counts.
                    if phase == .active { UNUserNotificationCenter.current().setBadgeCount(model.fleet.recordBadge()) }
                }
                .onChange(of: phase, initial: true) { _, phase in
                    switch phase {
                    case .active:
                        model.tokens.refreshSharedTokens()
                        model.fleet.activate()
                        model.nearby.start()
                        // Registering is idempotent: a computer that lost
                        // or never got this phone's tokens has them again.
                        Task { await model.tokens.sync() }
                    case .background:
                        model.fleet.deactivate()
                        model.nearby.stop()
                    default: break
                    }
                }
        }
        .environment(model)
        // A chat or terminal in a window of its own (iPad, Stage Manager).
        WindowGroup("Chat", for: Route.self) { $route in
            RouteWindow(route: route)
            .environment(model)
            .modifier(AppLock())
            .modifier(Appearance())
            .browsesLinks(othersToSystem: true)
        }
        .environment(model)
    }
}

/// A chat, terminal, workspace, or changes in a window of its own. What
/// opens from it, such as a `ScreenLink` tapped in its chat, pushes onto its
/// own stack (`windowRoutes`), as no root view there reads the app's
/// pending routes.
private struct RouteWindow: View {
    let route: Route?
    @State private var path: [Route] = []

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                switch route {
                case let .chat(key, title)?: ChatScreen(key: key, title: title)
                case let .term(key, title)?: TerminalScreen(key: key, title: title)
                case let .workspace(key)?: WorkspaceScreen(key: key)
                case let .changes(key)?: ChangesScreen(key: key)
                case nil: ContentUnavailableView("Nothing to show", systemImage: "questionmark.bubble")
                }
            }
            .withRoutes()
        }
        .environment(\.windowRoutes, WindowRoutes { path.append($0) })
    }
}
