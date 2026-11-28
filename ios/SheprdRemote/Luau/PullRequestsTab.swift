import SwiftUI

/// The Pull Requests tab as Settings › Developer picks it: the SwiftUI
/// list, or the same screen in Luau drawn by MoonKit. Switching is live,
/// with no relaunch.
///
/// Everything Luau in the app is behind this view and `ios/Luau`, so going
/// back to SwiftUI alone is one revert.
struct PullRequestsTab: View {
    @AppStorage(ScreenEngine.pullRequestsKey) private var engine = ScreenEngine.swift

    var body: some View {
        switch engine {
        case .swift: PullRequestsList()
        case .luau: LuauPullRequestsList()
        }
    }
}

/// What draws a screen being moved to Luau: its SwiftUI original, the
/// default, or its Luau version.
enum ScreenEngine: String, CaseIterable, Identifiable {
    case swift, luau

    var id: Self { self }

    /// Where the Pull Requests tab's choice is kept.
    static let pullRequestsKey = "pullRequestsEngine"

    var title: String {
        switch self {
        case .swift: "SwiftUI"
        case .luau: "Luau"
        }
    }
}

/// Settings › Developer: which engine draws the screens moved to Luau.
struct DeveloperSettingsSection: View {
    @AppStorage(ScreenEngine.pullRequestsKey) private var pullRequests = ScreenEngine.swift

    var body: some View {
        Section {
            Picker(selection: $pullRequests) {
                ForEach(ScreenEngine.allCases) { Text($0.title).tag($0) }
            } label: {
                Label { Text("PRs Tab") } icon: { SettingsIcon(symbol: "hammer.fill", color: .gray) }
            }
            .pickerStyle(.menu)
        } header: {
            Text("Developer")
        } footer: {
            Text("Draws the tab with its Luau version, to compare it with SwiftUI. It switches at once.")
        }
    }
}
