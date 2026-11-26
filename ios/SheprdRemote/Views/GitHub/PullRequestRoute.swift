import SheprdGitHub
import SheprdRemoteCore
import SwiftUI
import UIKit

/// A pull request a list opens, as a navigation value: where it lives, its
/// number, and its page on GitHub.
struct PullRequestRoute: Hashable {
    let host: GitHubHost
    let owner: String
    let repo: String
    let number: UInt64
    /// Its page, always `https` on its own host.
    let url: URL?

    init(host: GitHubHost, owner: String, repo: String, number: UInt64, url: URL?) {
        self.host = host
        self.owner = owner
        self.repo = repo
        self.number = number
        self.url = url
    }

    init(_ pull: GitHubSearchPull) {
        self.init(host: pull.host, owner: pull.owner, repo: pull.repo, number: pull.number, url: pull.url)
    }
}

extension PullRequestRoute: Identifiable {
    var id: Self { self }
}

extension AppModel {
    /// Opens the pull request a notification's Open PR names: in the app's
    /// own screen, which reads it with whatever GitHub token the phone has
    /// or offers GitHub's page; in Safari when the app cannot name its host.
    func openPullRequest(_ pull: AlertPullRequest) {
        guard let host = GitHubHost(pull.host) else {
            UIApplication.shared.open(pull.url)
            return
        }
        pendingPullRequest = PullRequestRoute(host: host, owner: pull.owner, repo: pull.repo, number: pull.number, url: pull.url)
    }
}

/// A pull request opened from a notification, over whatever is on screen.
struct PullRequestSheet: View {
    let route: PullRequestRoute
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            PullRequestDetailScreen(route: route, title: GitHubDirect.shared.title(for: route))
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Done") { dismiss() }
                    }
                }
        }
    }
}

extension GitHubSearchPull {
    /// Whether `route` names this pull request, its owner and repository in
    /// any case.
    func names(_ route: PullRequestRoute) -> Bool {
        host == route.host && number == route.number
            && owner.caseInsensitiveCompare(route.owner) == .orderedSame
            && repo.caseInsensitiveCompare(route.repo) == .orderedSame
    }
}

extension PullRequestDetailScreen {
    /// The detail screen a list row's route leads to.
    init(route: PullRequestRoute, title: String?) {
        self.init(
            target: PullRequestTarget(host: route.host, owner: route.owner, repo: route.repo, number: route.number),
            summary: PullRequestSummary(number: route.number, title: title, url: route.url?.absoluteString)
        )
    }
}
