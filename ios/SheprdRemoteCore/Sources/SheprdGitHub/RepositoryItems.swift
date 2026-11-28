import Foundation
import SheprdRemoteCore

/// An open pull request or issue of one repository, which a new workspace
/// may start from, as the desktop's new worktree dialog lists them.
public struct RepositoryItem: Hashable, Sendable, Identifiable {
    public enum Kind: String, Hashable, Sendable { case pull, issue }

    public var kind: Kind
    public var number: Int
    public var title: String
    public var author: String
    public var isDraft: Bool
    /// A pull request's head branch; `nil` for an issue.
    public var head: String?
    /// Who owns the repository a pull request's head lives in; `nil` when
    /// GitHub no longer says, as for a deleted fork.
    public var headOwner: String?

    public init(kind: Kind, number: Int, title: String, author: String = "", isDraft: Bool = false, head: String? = nil, headOwner: String? = nil) {
        self.kind = kind
        self.number = number
        self.title = title
        self.author = author
        self.isDraft = isDraft
        self.head = head
        self.headOwner = headOwner
    }

    public var id: String { "\(kind.rawValue)/\(number)" }
    /// What its workspace is named, as the desktop names it.
    public var label: String { "#\(number) \(title)" }

    /// Whether `query` finds it, by number, title, author, or branch.
    public func matches(_ query: String) -> Bool {
        let query = query.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return true }
        return [label, author, head ?? ""].contains { $0.localizedCaseInsensitiveContains(query) }
    }

    /// GitHub's "create a branch" name for an issue: its number, then a
    /// slug of its title, as the desktop names one (`issue_branch`).
    public var issueBranch: String {
        let slug = Self.slug(title, limit: 60)
        return slug.isEmpty ? "\(number)" : "\(number)-\(slug)"
    }

    /// Lowercase ASCII letters and digits, every other run one dash, at
    /// most `limit` long, as the desktop's `slug`.
    static func slug(_ text: String, limit: Int) -> String {
        var slug = ""
        var lastWasDash = false
        for scalar in text.unicodeScalars {
            if slug.count >= limit { break }
            if scalar.isASCII, CharacterSet.alphanumerics.contains(scalar) {
                slug.append(Character(scalar).lowercased())
                lastWasDash = false
            } else if !lastWasDash, !slug.isEmpty {
                slug.append("-")
                lastWasDash = true
            }
        }
        while slug.hasSuffix("-") { slug.removeLast() }
        return slug
    }
}

/// A repository's open pull requests and issues, most recently updated
/// first.
public struct RepositoryItems: Hashable, Sendable {
    public var pulls: [RepositoryItem]
    public var issues: [RepositoryItem]

    public init(pulls: [RepositoryItem] = [], issues: [RepositoryItem] = []) {
        self.pulls = pulls
        self.issues = issues
    }
}

/// What reading a repository's pull requests and issues came to.
public enum RepositoryItemsRead: Hashable, Sendable {
    case found(RepositoryItems)
    /// No token for the repository's host: Settings > GitHub fixes that.
    case noAccount
    case failed
}

/// The desktop's listing query (`repo_items` in `sheprd-gpui`): both lists
/// in one GraphQL request, the repository named by variables.
enum RepositoryItemsQuery {
    static let limit = 50

    static let text = """
    query($owner: String!, $repo: String!, $count: Int!) {
      repository(owner: $owner, name: $repo) {
        pullRequests(first: $count, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
          nodes { number title isDraft headRefName author { login } headRepositoryOwner { login } }
        }
        issues(first: $count, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
          nodes { number title author { login } }
        }
      }
    }
    """

    static func body(owner: String, repo: String) throws -> Data {
        try JSONSerialization.data(
            withJSONObject: ["query": text, "variables": ["owner": owner, "repo": repo, "count": limit]],
            options: [.sortedKeys]
        )
    }

    struct Answer: Decodable {
        var data: DataField?
        var errors: [PullRequestAnswer.Answer.Failure]?

        struct DataField: Decodable { var repository: Repository? }
        struct Repository: Decodable {
            var pullRequests: List?
            var issues: List?
        }
        struct List: Decodable { var nodes: [Node?] }
        struct Login: Decodable { var login: String }
        struct Node: Decodable {
            var number: Int
            var title: String?
            var isDraft: Bool?
            var headRefName: String?
            var author: Login?
            var headRepositoryOwner: Login?
        }
    }

    /// The lists in `response`. A repository the token cannot see comes
    /// back as a null node, which is GitHub's not found.
    static func parse(_ response: GitHubResponse, now: Date) throws(GitHubError) -> RepositoryItems {
        guard let answer = try? JSONDecoder().decode(Answer.self, from: response.body) else { throw .malformed }
        guard let repository = answer.data?.repository else {
            let errors = answer.errors ?? []
            throw errors.isEmpty ? .notFound : PullRequestAnswer.failure(errors, response: response, now: now)
        }
        return RepositoryItems(
            pulls: items(repository.pullRequests, kind: .pull),
            issues: items(repository.issues, kind: .issue)
        )
    }

    private static func items(_ list: Answer.List?, kind: RepositoryItem.Kind) -> [RepositoryItem] {
        (list?.nodes ?? []).prefix(limit).compactMap { node in
            guard let node, node.number > 0 else { return nil }
            let head = PullRequestSearchAnswer.branch(node.headRefName)
            // A pull request without a usable head cannot seed a checkout.
            if kind == .pull, head == nil { return nil }
            return RepositoryItem(
                kind: kind,
                number: node.number,
                title: PullRequestAnswer.clean(node.title ?? ""),
                author: PullRequestSearchAnswer.name(node.author?.login) ?? "",
                isDraft: node.isDraft ?? false,
                head: kind == .pull ? head : nil,
                headOwner: kind == .pull ? PullRequestSearchAnswer.name(node.headRepositoryOwner?.login) : nil
            )
        }
    }
}

extension GitHubAPI {
    func repositoryItems(host: GitHubHost, owner: String, repo: String, token: GitHubToken) async throws -> RepositoryItems {
        var request = GitHubHTTP.request(host.graphQL, token: token, method: "POST")
        request.httpBody = try RepositoryItemsQuery.body(owner: owner, repo: repo)
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let response = try await send(request, host: host, limit: .whole(max: Self.graphQLLimit))
        guard response.status == 200 else { throw GitHubHTTP.failure(response, now: now()) }
        do {
            return try RepositoryItemsQuery.parse(response, now: now())
        } catch {
            if case let .rateLimited(until) = error { paused[host] = until }
            throw error
        }
    }
}

extension PullRequestReader {
    /// `owner/repo`'s open pull requests and issues on `host`, with the
    /// phone's own sign-in first on github.com, then a lent token.
    public func repositoryItems(host name: String, owner: String, repo: String, source: Lenders) async -> RepositoryItemsRead {
        guard let host = GitHubHost(name),
              let owner = PullRequestSearchAnswer.name(owner),
              let repo = PullRequestSearchAnswer.name(repo)
        else { return .failed }
        guard await hasAccount(for: host, source: source) else { return .noAccount }
        let found = await attempt(host: host, source: source) { api, token in
            try await api.repositoryItems(host: host, owner: owner, repo: repo, token: token)
        }
        return found.map(RepositoryItemsRead.found) ?? .failed
    }
}
