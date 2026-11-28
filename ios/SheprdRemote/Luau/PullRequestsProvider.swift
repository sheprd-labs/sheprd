import Foundation
import MoonKit
import Observation
import SheprdGitHub
import SheprdRemoteCore
import UIKit

/// The Pull Requests tab's model as Luau reads it: `app.pulls`, with the
/// lists already searched and sorted by the same Swift as the SwiftUI tab.
/// Luau draws it and asks for things by sending to `provider:pulls`; it
/// has no network and opens nothing itself.
///
/// Requests: `{ query = text }`, `{ sort = "updated" | "status" }`,
/// `{ refresh = true }`, `{ github = true }` for Settings › GitHub,
/// `{ device = id, workspace = id }` to open a workspace carrying a pull
/// request, and `{ web = id }` to open its page.
@MainActor
final class PullRequestsProvider: Provider {
    let name = "pulls"

    /// Where the host view opens what Luau asks for.
    var openWorkspace: (WorkspaceKey) -> Void = { _ in }
    var openPage: (URL) -> Void = { _ in }
    var openGitHubSettings: () -> Void = {}

    private let model: AppModel
    private var link: ProviderLink?
    private var query = ""
    private var refreshing = false
    /// What was last published, for requests naming a pull request.
    private var last = PullRequestsSnapshot()

    private var direct: GitHubDirect { GitHubDirect.shared }

    init(model: AppModel) {
        self.model = model
        #if DEBUG
        if let text = UserDefaults.standard.string(forKey: "pullRequestSearch") { query = text }
        #endif
    }

    func start(_ link: ProviderLink) {
        self.link = link
        follow()
    }

    func receive(_ message: LuauValue, from mod: String?) {
        if let text = message["query"]?.string {
            query = text
        } else if let sort = message["sort"]?.string.flatMap(PullRequestSort.init(rawValue:)) {
            UserDefaults.standard.set(sort.rawValue, forKey: PullRequestSort.key)
        } else if message["refresh"] != nil {
            refresh()
        } else if message["github"] != nil {
            openGitHubSettings()
        } else if let device = message["device"]?.string, let workspace = message["workspace"]?.string,
                  !device.isEmpty, !workspace.isEmpty {
            // Named, not by position, so a list that changed while the menu
            // was open still opens the workspace chosen.
            openWorkspace(WorkspaceKey(device: device, workspace: workspace))
        } else if let id = message["web"]?.string, let url = last.pulls[id]?.url {
            openPage(url)
        }
        publish()
    }

    /// The route a row's `{ pull = id }` link opens.
    func route(for id: String) -> PullRequestRoute? {
        last.pulls[id].map(PullRequestRoute.init)
    }

    /// Publishes now, and again whenever what it read changes.
    private func follow() {
        withObservationTracking { publish() } onChange: { [weak self] in
            Task { @MainActor in self?.follow() }
        }
    }

    private func refresh() {
        guard !refreshing else { return }
        refreshing = true
        Task {
            await direct.refreshLists(source: model.fleet.gitHubLenders, states: filter.states)
            refreshing = false
            publish()
        }
    }

    /// The PRs tab's filter, as the SwiftUI tab keeps it: the states read
    /// are the ones it shows.
    private var filter: PullRequestFilter {
        UserDefaults.standard.string(forKey: PullRequestFilter.key).flatMap(PullRequestFilter.init(rawValue:)) ?? PullRequestFilter()
    }

    private var sort: PullRequestSort {
        UserDefaults.standard.string(forKey: PullRequestSort.key).flatMap(PullRequestSort.init(rawValue:)) ?? .updated
    }

    private var computers: [ComputerWorkspaces] {
        #if DEBUG
        if GitHubSample.enabled { return GitHubSample.computers }
        #endif
        return model.fleet.computerWorkspaces
    }

    private func publish() {
        guard let link else { return }
        last = PullRequestsSnapshot(
            lists: direct.lists, loading: direct.loadingLists, refreshing: refreshing, query: query, sort: sort,
            device: UIDevice.current.model, computers: computers
        )
        link.publish(last.value)
    }
}

/// One publication of `app.pulls`: the lists as shown, searched and sorted,
/// and the pull requests its rows name by id.
struct PullRequestsSnapshot {
    private(set) var value = LuauValue.emptyTable
    private(set) var pulls: [String: GitHubSearchPull] = [:]

    init() {}

    init(
        lists: GitHubDirect.Lists, loading: Bool, refreshing: Bool, query: String, sort: PullRequestSort,
        device: String, computers: [ComputerWorkspaces], now: Date = .now
    ) {
        let shown = lists.lists?.matching(query).sorted(sort)
        let requested = shown?.reviewRequested ?? [], mine = shown?.mine ?? []
        var rows: [String: LuauValue] = [:]
        for pull in requested + mine where pulls[pull.id] == nil {
            let found = ComputerWorkspaces.places(of: pull, in: computers)
            pulls[pull.id] = pull
            rows[pull.id] = Self.row(pull, places: found, now: now)
        }
        value = .fields([
            "status": .string(Self.status(lists)),
            "loading": .bool(loading),
            "refreshing": .bool(refreshing),
            "query": .string(query),
            "sort": .string(sort.rawValue),
            "device": .string(device),
            "any": .bool(!(lists.lists?.isEmpty ?? true)),
            "requested": .table(array: requested.map { .string($0.id) }, fields: [:]),
            "mine": .table(array: mine.map { .string($0.id) }, fields: [:]),
            "requestedTotal": .number(Double(shown?.reviewRequestedTotal ?? 0)),
            "mineTotal": .number(Double(shown?.mineTotal ?? 0)),
            "rows": .fields(rows),
        ])
    }

    private static func status(_ lists: GitHubDirect.Lists) -> String {
        switch lists {
        case .unread: "unread"
        case .noAccount: "noAccount"
        case .failed: "failed"
        case .read: "read"
        }
    }

    private static func row(_ pull: GitHubSearchPull, places: [WorkspacePlace], now: Date) -> LuauValue {
        var fields: [String: LuauValue] = [
            "id": .string(pull.id),
            "repository": .string("\(pull.repository) #\(pull.number)"),
            "title": .string(pull.title),
            "age": .string(PullRequestLink.age(pull.updatedAt, now: now)),
            "state": .string(state(pull)),
            "checks": .fields([
                "failing": .number(Double(pull.checks.failing)),
                "pending": .number(Double(pull.checks.pending)),
                "passed": .number(Double(pull.checks.passed)),
            ]),
            "places": .table(array: places.map {
                .fields(["computer": .string($0.computer), "device": .string($0.device), "workspace": .string($0.workspace)])
            }, fields: [:]),
            "computers": .number(Double(Set(places.map(\.device)).count)),
        ]
        if let review = review(pull.reviewDecision) { fields["review"] = .string(review) }
        if let author = pull.author { fields["author"] = .string(author) }
        if let url = pull.url { fields["url"] = .string(url.absoluteString) }
        return .fields(fields)
    }

    private static func state(_ pull: GitHubSearchPull) -> String {
        switch pull.state {
        case .open: pull.isDraft ? "draft" : "open"
        case .merged: "merged"
        case .closed: "closed"
        case .unknown: "unknown"
        }
    }

    private static func review(_ decision: GitHubPullRequest.ReviewDecision) -> String? {
        switch decision {
        case .approved: "approved"
        case .changesRequested: "changesRequested"
        case .reviewRequired: "reviewRequired"
        case .none: nil
        }
    }
}
