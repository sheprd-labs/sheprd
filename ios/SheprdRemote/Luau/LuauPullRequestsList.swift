import MoonKit
import MoonUI
import SwiftUI

/// The Pull Requests tab drawn by its Luau, `ios/Luau/pull-requests`, from
/// the same model as `PullRequestsList`. Luau draws the list; Swift keeps
/// reading GitHub, opens what a row asks for, and pushes the same screens.
///
/// Only bundled, fixed Luau runs here: the app links MoonKit and MoonUI,
/// never MoonLive, so nothing can change it while it runs
/// (`ios/tools/luau_store_audit.sh`).
struct LuauPullRequestsList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL
    /// Made on the first draw, not in a task, so the first frame has the list.
    @State private var store = LuauScreenStore()
    /// A workspace opened from a row's menu.
    @State private var opened: WorkspaceKey?
    @State private var showingGitHub = false

    var body: some View {
        let screen = store.screen(model: model)
        MoonCanvas()
            .environment(screen.workspace)
            .onAppear {
                // Bindings and the action, not the view: the view holds the
                // store, which holds the provider, so capturing it would
                // keep a switched-away screen alive.
                let (opened, showingGitHub, openURL) = ($opened, $showingGitHub, openURL)
                screen.provider.openWorkspace = { opened.wrappedValue = $0 }
                screen.provider.openPage = { openURL($0) }
                screen.provider.openGitHubSettings = { showingGitHub.wrappedValue = true }
            }
            .navigationBarTitleDisplayMode(.inline)
            .gitHubRefresh(showsLists: true)
            .navigationDestination(for: LuauValue.self) { route in
                destination(route)
            }
            .navigationDestination(item: $opened) { WorkspaceScreen(key: $0).makesRoomForConnectionToast() }
            .navigationDestination(isPresented: $showingGitHub) { GitHubSettingsView().makesRoomForConnectionToast() }
    }

    /// What a row's Luau link opens: `{ pull = id }`.
    @ViewBuilder private func destination(_ route: LuauValue) -> some View {
        if let id = route["pull"]?.string, let route = store.loaded?.provider.route(for: id) {
            PullRequestDetailScreen(route: route, title: GitHubDirect.shared.title(for: route))
                .makesRoomForConnectionToast()
        } else {
            ContentUnavailableView("This pull request is gone", systemImage: "arrow.triangle.pull")
        }
    }
}

/// Holds the screen once made. Not observed: making it is not a change
/// the view needs to hear about.
@MainActor
private final class LuauScreenStore {
    private(set) var loaded: LuauScreen?

    func screen(model: AppModel) -> LuauScreen {
        if let loaded { return loaded }
        let made = LuauScreen(model: model)
        loaded = made
        return made
    }
}

/// The tab's Luau, loaded from the bundle, and the provider it reads.
@MainActor
private final class LuauScreen {
    let workspace: Workspace
    let provider: PullRequestsProvider

    init(model: AppModel) {
        let folder = Bundle.main.url(forResource: "Luau/pull-requests", withExtension: nil)
            ?? Bundle.main.bundleURL.appending(path: "Luau/pull-requests")
        #if DEBUG
        // An agent's files over the bundled ones (`sheprd phone luau`).
        workspace = Workspace(folders: LuauLive.folders("pull-requests", bundled: folder))
        LuauLive.shared.register(workspace, screen: "pull-requests")
        #else
        workspace = Workspace(folders: Folders(builtin: folder))
        #endif
        provider = PullRequestsProvider(model: model)
        workspace.attach(provider)
        workspace.reload()
    }
}
