import Foundation
import SheprdRemoteCore

/// A branch's pull request as the Changes screen's header shows it: the
/// whole pull request when GitHub answered, only its checks (and their
/// number) when the computer did.
public struct BranchPullRequest: Hashable, Sendable {
    public var checks: PullRequestChecks
    /// Title, state, and review; nil when the computer answered, or when
    /// the branch has none.
    public var pullRequest: GitHubPullRequest?

    public init(checks: PullRequestChecks, pullRequest: GitHubPullRequest? = nil) {
        self.checks = checks
        self.pullRequest = pullRequest
    }
}

/// The pull request a detail screen reads: where it lives and its number.
public struct PullRequestTarget: Hashable, Sendable {
    public var host: GitHubHost
    public var owner: String
    public var repo: String
    public var number: UInt64

    public init(host: GitHubHost, owner: String, repo: String, number: UInt64) {
        self.host = host
        self.owner = owner
        self.repo = repo
        self.number = number
    }

    /// `workspace`'s pull request numbered `number`, in the repository its
    /// computer names; nil when it names none on a host the phone knows.
    public init?(workspace: Home.Workspace, number: UInt64) {
        guard let github = workspace.github, let host = GitHubHost(github.host) else { return nil }
        self.init(host: host, owner: github.owner, repo: github.repo, number: number)
    }
}

/// What a detail read found: the pull request, its files unless that second
/// read failed, and the GitHub login whose token read it (so viewer-scoped
/// actions like toggling reactions stay bound to that same account).
public struct PullRequestReading: Hashable, Sendable {
    public var detail: PullRequestDetail
    public var files: PullRequestFiles?
    public var viewerLogin: String?

    public init(detail: PullRequestDetail, files: PullRequestFiles?, viewerLogin: String? = nil) {
        self.detail = detail
        self.files = files
        self.viewerLogin = viewerLogin
    }

    /// Whether `other` was read by the same GitHub account (case-insensitive).
    public func sameViewer(as other: PullRequestReading) -> Bool {
        switch (viewerLogin, other.viewerLogin) {
        case (nil, nil): true
        case let (a?, b?): a.caseInsensitiveCompare(b) == .orderedSame
        default: false
        }
    }
}

extension PullRequestReader {
    /// `workspace`'s pull request, read from GitHub with the same tokens
    /// as `checks`, else its checks from the computer.
    public func branchPullRequest(
        workspace: Home.Workspace,
        computer: String,
        source: Lenders,
        askComputer: @Sendable () async throws -> PullRequestChecks
    ) async throws -> Sourced<BranchPullRequest> {
        let found = await pullRequests(workspaces: [workspace], source: source)
        if let direct = found[workspace.id] {
            let checks = direct?.checkList ?? PullRequestChecks(checks: [])
            return Sourced(BranchPullRequest(checks: checks, pullRequest: direct), from: .github)
        }
        return Sourced(BranchPullRequest(checks: try await askComputer()), from: .computer(computer))
    }

    /// `target`'s detail and files, straight from GitHub; nil when no
    /// token reads it. The computer has no conversation to fall back on,
    /// so the screen then offers GitHub's page instead.
    public func detail(_ target: PullRequestTarget, source: Lenders) async -> PullRequestReading? {
        await attemptWithLogin(host: target.host, source: source) { api, token, login in
            let detail = try await api.pullRequestDetail(
                host: target.host, owner: target.owner, repo: target.repo, number: target.number, token: token
            )
            // The conversation still shows when the files cannot be read,
            // but a refused token goes back so it is renewed and retried.
            let files: PullRequestFiles?
            do {
                files = try await api.pullRequestFiles(
                    host: target.host, owner: target.owner, repo: target.repo, number: target.number, token: token
                )
            } catch GitHubError.unauthorized {
                throw GitHubError.unauthorized
            } catch {
                files = nil
            }
            return PullRequestReading(detail: detail, files: files, viewerLogin: login.isEmpty ? nil : login)
        }
    }

    /// Adds or removes an emoji reaction on `subjectId` and returns the
    /// updated reaction groups, bound to `asLogin` so fallback never mutates
    /// a different GitHub account's reaction.
    public func setReaction(
        _ content: PullRequestDetail.ReactionContent,
        add: Bool,
        on subjectId: String,
        host: GitHubHost,
        asLogin: String? = nil,
        source: Lenders
    ) async -> Result<[PullRequestDetail.ReactionGroup], PullRequestReactionError> {
        let result = await mutate(host: host, asLogin: asLogin, source: source) { api, token in
            try await api.setReaction(host: host, subjectId: subjectId, content: content, add: add, token: token)
        }
        return result.mapError(PullRequestReactionError.init)
    }
}
