import Foundation

/// A GraphQL mutation that adds or removes an emoji reaction on a GitHub
/// subject (such as a pull request description) and reads back the updated
/// reaction groups. Identifiers travel as variables, never interpolated.
struct PullRequestReactionMutation: Sendable, Equatable {
    var subjectId: String
    var content: PullRequestDetail.ReactionContent
    var add: Bool

    var text: String {
        add ? Self.addText : Self.removeText
    }

    var body: Data {
        get throws {
            let variables: [String: Any] = ["subjectId": subjectId, "content": content.rawValue]
            return try JSONSerialization.data(withJSONObject: ["query": text, "variables": variables], options: [.sortedKeys])
        }
    }

    static let addText = """
    mutation($subjectId: ID!, $content: ReactionContent!) {
      addReaction(input: { subjectId: $subjectId, content: $content }) {
        subject { reactionGroups { content viewerHasReacted reactors { totalCount } } }
      }
    }
    """

    static let removeText = """
    mutation($subjectId: ID!, $content: ReactionContent!) {
      removeReaction(input: { subjectId: $subjectId, content: $content }) {
        subject { reactionGroups { content viewerHasReacted reactors { totalCount } } }
      }
    }
    """
}

/// Reading a `PullRequestReactionMutation`'s answer.
enum PullRequestReactionAnswer {
    static func parse(
        _ response: GitHubResponse,
        add: Bool,
        now: Date
    ) throws(GitHubError) -> [PullRequestDetail.ReactionGroup] {
        guard let answer = try? JSONDecoder().decode(Answer.self, from: response.body) else { throw .malformed }
        let payload = add ? answer.data?.addReaction : answer.data?.removeReaction
        guard let groups = payload?.subject?.reactionGroups else {
            let errors = answer.errors ?? []
            throw errors.isEmpty ? .malformed : PullRequestAnswer.failure(errors, response: response, now: now)
        }
        return PullRequestDetailAnswer.parseReactions(groups)
    }

    private struct Answer: Decodable {
        var data: DataField?
        var errors: [PullRequestAnswer.Answer.Failure]?

        struct DataField: Decodable {
            var addReaction: Payload?
            var removeReaction: Payload?
        }

        struct Payload: Decodable {
            var subject: Subject?
        }

        struct Subject: Decodable {
            var reactionGroups: [PullRequestDetailAnswer.ReactionGroupNode?]?
        }
    }
}

/// Why adding or removing a pull request reaction failed.
public enum PullRequestReactionError: Error, Equatable, Sendable, LocalizedError {
    case noAccount
    case forbidden
    case ssoRequired
    case rateLimited
    case failed

    public var errorDescription: String? {
        switch self {
        case .noAccount:
            "Sign in to GitHub or share a computer's sign-in to react."
        case .forbidden:
            "GitHub did not allow reacting with this sign-in."
        case .ssoRequired:
            "Authorize SSO for this organization on GitHub to react."
        case .rateLimited:
            "GitHub rate limit reached. Try again later."
        case .failed:
            "Could not update the reaction. Try again."
        }
    }

    init(_ error: GitHubError) {
        switch error {
        case .unauthorized: self = .noAccount
        case .forbidden: self = .forbidden
        case .ssoRequired: self = .ssoRequired
        case .rateLimited: self = .rateLimited
        case .notFound, .status, .malformed, .tooLarge, .location, .ambiguous: self = .failed
        }
    }
}

/// Coordinates optimistic PR description reaction toggles against periodic
/// refreshes and concurrent mutations so older answers never overwrite newer
/// reaction state.
public struct PullRequestReactionTracker: Equatable, Sendable {
    private var inFlight: [PullRequestDetail.ReactionContent: Int] = [:]
    /// Monotonic counter bumped whenever a toggle starts, finishes, or resets.
    private var version = 0

    public init() {}

    /// Reactions currently being written to GitHub.
    public var pending: Set<PullRequestDetail.ReactionContent> {
        Set(inFlight.keys)
    }

    /// Snapshot captured when a detail refresh starts.
    public struct RefreshToken: Equatable, Sendable {
        fileprivate let version: Int
        fileprivate let startedIdle: Bool
    }

    /// Token identifying one in-flight toggle, so a stale task finishing after
    /// a reset cannot clear a newer toggle for the same emoji.
    public struct ToggleToken: Equatable, Sendable {
        public let content: PullRequestDetail.ReactionContent
        fileprivate let id: Int
    }

    /// Captures the current reaction version before starting a refresh.
    public func beginRefresh() -> RefreshToken {
        RefreshToken(version: version, startedIdle: inFlight.isEmpty)
    }

    /// Whether a refresh started with `token` is still fresh enough to replace
    /// the displayed reactions: no write was in flight when it started, none
    /// ran while it was in flight, and none is in flight now.
    public func acceptsRefresh(_ token: RefreshToken) -> Bool {
        token.startedIdle && token.version == version && inFlight.isEmpty
    }

    /// Begins an optimistic toggle for `content`; nil if one is already in flight.
    public mutating func beginToggle(_ content: PullRequestDetail.ReactionContent) -> ToggleToken? {
        guard inFlight[content] == nil else { return nil }
        version += 1
        inFlight[content] = version
        return ToggleToken(content: content, id: version)
    }

    /// Whether `token` is still the active in-flight toggle (not invalidated by
    /// a reset or superseded by a newer toggle).
    public func isActive(_ token: ToggleToken) -> Bool {
        inFlight[token.content] == token.id
    }

    /// Ends the toggle identified by `token`, ignoring stale tokens from before a reset.
    @discardableResult
    public mutating func finishToggle(_ token: ToggleToken) -> Bool {
        guard inFlight[token.content] == token.id else { return false }
        inFlight.removeValue(forKey: token.content)
        version += 1
        return true
    }

    /// Drops all in-flight tracking (for example after sign-out or viewer switch).
    public mutating func reset() {
        inFlight.removeAll()
        version += 1
    }

    /// Merges the authoritative server result for `content` into `current`
    /// without overwriting any other reaction, so out-of-order concurrent
    /// writes for different emojis never clobber one another.
    public static func apply(
        _ serverGroups: [PullRequestDetail.ReactionGroup],
        for content: PullRequestDetail.ReactionContent,
        into current: [PullRequestDetail.ReactionGroup]
    ) -> [PullRequestDetail.ReactionGroup] {
        var merged = current
        if let updated = serverGroups.first(where: { $0.content == content }) {
            if let index = merged.firstIndex(where: { $0.content == content }) {
                merged[index] = updated
            } else {
                merged.append(updated)
            }
        } else {
            merged.removeAll { $0.content == content }
        }
        return merged
    }
}
