import Foundation

/// What the Pull Requests tab lists: pull requests in some states, its
/// two sections or one, and every repository or one owner's or one.
/// The states decide what is read from GitHub; the rest narrows what was
/// read. Kept on the phone as JSON.
public struct PullRequestFilter: Hashable, Sendable, Codable {
    /// Where a pull request stands, as the filter tells them apart: a
    /// draft is open on GitHub, a closed one was not merged.
    public enum State: String, CaseIterable, Hashable, Sendable, Codable, Identifiable {
        case open, draft, merged, closed

        public var id: Self { self }

        /// Where `pull` stands; nil for a state GitHub added since.
        public init?(of pull: GitHubSearchPull) {
            switch pull.state {
            case .open: self = pull.isDraft ? .draft : .open
            case .merged: self = .merged
            case .closed: self = .closed
            case .unknown: return nil
            }
        }
    }

    /// The states the tab listed before it had a filter.
    public static let openStates: Set<State> = [.open, .draft]

    /// Never empty: a filter with none lists the open ones.
    public var states: Set<State> {
        didSet { if states.isEmpty { states = Self.openStates } }
    }
    public var showsReviewRequested: Bool
    public var showsMine: Bool
    /// `owner` or `owner/repo`, lowercased; nil for every repository.
    public var repository: String?

    public init(states: Set<State> = openStates, showsReviewRequested: Bool = true, showsMine: Bool = true, repository: String? = nil) {
        self.states = states.isEmpty ? Self.openStates : states
        self.showsReviewRequested = showsReviewRequested
        self.showsMine = showsMine
        self.repository = repository?.lowercased()
    }

    /// Where the person keeps it.
    public static let key = "pullRequestFilter"

    /// Whether it lists what the tab lists unfiltered: the open pull
    /// requests of both sections, on every repository.
    public var isStandard: Bool {
        states == Self.openStates && showsReviewRequested && showsMine && repository == nil
    }

    /// Whether `pull` is listed, whatever its section.
    public func lists(_ pull: GitHubSearchPull) -> Bool {
        guard let state = State(of: pull), states.contains(state) else { return false }
        guard let repository else { return true }
        let name = pull.repository.lowercased()
        return name == repository || name.hasPrefix("\(repository)/")
    }
}

extension PullRequestFilter {
    private enum CodingKeys: String, CodingKey {
        case states, showsReviewRequested, showsMine, repository
    }

    /// What is missing reads as unfiltered, so a filter kept by an older
    /// build still applies.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            states: try container.decodeIfPresent(Set<State>.self, forKey: .states) ?? Self.openStates,
            showsReviewRequested: try container.decodeIfPresent(Bool.self, forKey: .showsReviewRequested) ?? true,
            showsMine: try container.decodeIfPresent(Bool.self, forKey: .showsMine) ?? true,
            repository: try container.decodeIfPresent(String.self, forKey: .repository)
        )
    }

    /// Written out rather than left to `RawRepresentable`'s default,
    /// which would encode `rawValue`, which encodes this, forever.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(states.map(\.rawValue).sorted(), forKey: .states)
        try container.encode(showsReviewRequested, forKey: .showsReviewRequested)
        try container.encode(showsMine, forKey: .showsMine)
        try container.encodeIfPresent(repository, forKey: .repository)
    }
}

extension PullRequestFilter: RawRepresentable {
    public init?(rawValue: String) {
        guard let filter = try? JSONDecoder().decode(Self.self, from: Data(rawValue.utf8)) else { return nil }
        self = filter
    }

    /// With its keys sorted: `==` on a `RawRepresentable` compares these,
    /// so equal filters must write the same text.
    public var rawValue: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        return (try? encoder.encode(self)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }
}

extension PullRequestLists {
    /// The pull requests `filter` lists, a hidden section empty. A list
    /// that loses any keeps what is left as its total, as a search does.
    public func filtered(_ filter: PullRequestFilter) -> PullRequestLists {
        func keep(_ pulls: [GitHubSearchPull], _ total: Int, shown: Bool) -> ([GitHubSearchPull], Int) {
            guard shown else { return ([], 0) }
            let kept = pulls.filter(filter.lists)
            return (kept, kept.count == pulls.count ? total : kept.count)
        }
        var filtered = self
        (filtered.mine, filtered.mineTotal) = keep(mine, mineTotal, shown: filter.showsMine)
        (filtered.reviewRequested, filtered.reviewRequestedTotal) = keep(reviewRequested, reviewRequestedTotal, shown: filter.showsReviewRequested)
        return filtered
    }

    /// The repositories the lists hold, by owner, both sorted without case.
    public var owners: [(owner: String, repositories: [String])] {
        let names = Set((mine + reviewRequested).map { $0.repository.lowercased() })
        let grouped = Dictionary(grouping: names) { String($0.prefix { $0 != "/" }) }
        return grouped.keys.sorted().map { owner in (owner, grouped[owner, default: []].sorted()) }
    }
}
