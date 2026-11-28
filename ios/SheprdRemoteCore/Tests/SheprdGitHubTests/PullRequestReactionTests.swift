import Foundation
import Testing
@testable import SheprdGitHub
@testable import SheprdRemoteCore

struct PullRequestReactionTests {
    private let transport = FakeTransport()
    private let lenders = FakeLenders()
    private let store = TokenStore(vault: .memory())

    private func api() -> GitHubAPI {
        GitHubAPI(transport: transport.transport, now: { GitHubFixtures.start })
    }

    private func reader(signIn: GitHubSignIn? = nil) -> PullRequestReader {
        PullRequestReader(leases: GitHubLeases(store: store, now: { GitHubFixtures.start }), api: api(), signIn: signIn)
    }

    private var source: Lenders { Lenders(lenders: [GitHubFixtures.lender("mac")], fetch: lenders.fetch) }

    private func reactionResponse(add: Bool, groups: [[String: Any]]) throws -> GitHubResponse {
        let field = add ? "addReaction" : "removeReaction"
        let body = try JSONSerialization.data(withJSONObject: [
            "data": [field: ["subject": ["reactionGroups": groups]]],
        ])
        return GitHubResponse(status: 200, body: body)
    }

    @Test func mutationSendsSubjectIdAndContentAsVariables() throws {
        let add = PullRequestReactionMutation(subjectId: "PR_\"injected", content: .rocket, add: true)
        let sent = try #require(try JSONSerialization.jsonObject(with: add.body) as? [String: Any])
        let variables = try #require(sent["variables"] as? [String: Any])
        #expect(variables["subjectId"] as? String == "PR_\"injected")
        #expect(variables["content"] as? String == "ROCKET")
        let query = try #require(sent["query"] as? String)
        #expect(query.contains("addReaction"))
        #expect(!query.contains("injected"))

        let remove = PullRequestReactionMutation(subjectId: "PR_1", content: .thumbsUp, add: false)
        let removeSent = try #require(try JSONSerialization.jsonObject(with: remove.body) as? [String: Any])
        let removeQuery = try #require(removeSent["query"] as? String)
        #expect(removeQuery.contains("removeReaction"))
    }

    @Test func apiRejectsInvalidSubjectIdWithoutSending() async {
        let api = api()
        await #expect(throws: GitHubError.malformed) {
            try await api.setReaction(host: GitHubFixtures.host, subjectId: "bad id\n", content: .thumbsUp, add: true, token: GitHubToken("t"))
        }
        #expect(transport.requests.isEmpty)
    }

    @Test func addAndRemoveParseUpdatedReactionGroups() async throws {
        lenders.answer("mac", GitHubFixtures.answer("t1"))
        transport.enqueue(try reactionResponse(add: true, groups: [
            ["content": "THUMBS_UP", "viewerHasReacted": true, "reactors": ["totalCount": 2]],
            ["content": "HEART", "viewerHasReacted": false, "reactors": ["totalCount": 0]],
            ["content": "THUMBS_UP", "viewerHasReacted": true, "reactors": ["totalCount": 99]],
        ]))
        let added = await reader().setReaction(.thumbsUp, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(added == .success([.init(content: .thumbsUp, count: 2, viewerHasReacted: true)]))

        transport.enqueue(try reactionResponse(add: false, groups: [
            ["content": "THUMBS_UP", "viewerHasReacted": false, "reactors": ["totalCount": 1]],
        ]))
        let removed = await reader().setReaction(.thumbsUp, add: false, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(removed == .success([.init(content: .thumbsUp, count: 1, viewerHasReacted: false)]))
    }

    @Test func refusedTokenRenewsOnce() async throws {
        lenders.answer("mac", GitHubFixtures.answer("t1"), GitHubFixtures.answer("t2"))
        transport.enqueue(GitHubResponse(status: 401))
        transport.enqueue(try reactionResponse(add: true, groups: [
            ["content": "EYES", "viewerHasReacted": true, "reactors": ["totalCount": 1]],
        ]))
        let result = await reader().setReaction(.eyes, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(result == .success([.init(content: .eyes, count: 1, viewerHasReacted: true)]))
        #expect(transport.requests.map { $0.value(forHTTPHeaderField: "Authorization") } == ["Bearer t1", "Bearer t2"])
    }

    @Test func failuresMapToTypedReactionErrors() async throws {
        let missing = await reader().setReaction(.thumbsUp, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(missing == .failure(.noAccount))

        lenders.answer("mac", GitHubFixtures.answer("t1"))
        transport.enqueue(GitHubResponse(
            status: 200,
            body: Data(#"{"data":{"addReaction":null},"errors":[{"type":"FORBIDDEN","message":"Resource not accessible by integration"}]}"#.utf8)
        ))
        let forbidden = await reader().setReaction(.thumbsUp, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(forbidden == .failure(.forbidden))

        transport.enqueue(GitHubResponse(
            status: 200,
            headers: ["X-GitHub-SSO": "required; url=https://github.com/orgs/acme/sso"],
            body: Data(#"{"data":{"addReaction":null},"errors":[{"type":"FORBIDDEN","message":"SSO required"}]}"#.utf8)
        ))
        let sso = await reader().setReaction(.thumbsUp, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(sso == .failure(.ssoRequired))

        transport.enqueue(GitHubResponse(
            status: 200,
            headers: ["Retry-After": "120"],
            body: Data(#"{"data":null,"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}]}"#.utf8)
        ))
        let limited = await reader().setReaction(.thumbsUp, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, source: source)
        #expect(limited == .failure(.rateLimited))
    }

    @Test func ownSignInFallsBackToLentTokenWhenSameLoginAndForbidden() async throws {
        let signInStore = GitHubSignInStore(vault: .memory())
        try signInStore.save(SignInFixtures.credential(login: "ana"))
        let signIn = GitHubSignIn(
            flow: GitHubDeviceFlow(transport: FakeTransport().transport, now: { GitHubFixtures.start }),
            store: signInStore
        )
        lenders.answer("mac", GitHubFixtures.answer("lent-token"))
        transport.enqueue(GitHubResponse(
            status: 200,
            body: Data(#"{"data":{"addReaction":null},"errors":[{"type":"FORBIDDEN","message":"Resource not accessible by integration"}]}"#.utf8)
        ))
        transport.enqueue(try reactionResponse(add: true, groups: [
            ["content": "HOORAY", "viewerHasReacted": true, "reactors": ["totalCount": 1]],
        ]))
        let result = await reader(signIn: signIn).setReaction(
            .hooray, add: true, on: "PR_kwDO42", host: GitHubFixtures.host, asLogin: "ANA", source: source
        )
        #expect(result == .success([.init(content: .hooray, count: 1, viewerHasReacted: true)]))
        #expect(transport.requests.map { $0.value(forHTTPHeaderField: "Authorization") } == ["Bearer ghu_a1", "Bearer lent-token"])
    }

    @Test func fallbackRefusesDifferentAccountLogin() async throws {
        let signInStore = GitHubSignInStore(vault: .memory())
        try signInStore.save(SignInFixtures.credential(login: "ana"))
        let signIn = GitHubSignIn(
            flow: GitHubDeviceFlow(transport: FakeTransport().transport, now: { GitHubFixtures.start }),
            store: signInStore
        )
        let bobLease = LeaseFetch.lease(LeaseAnswer(
            host: "github.com", login: "bob", token: GitHubToken("bob-token"), source: .gh
        ))
        lenders.answer("mac", bobLease)
        transport.enqueue(GitHubResponse(
            status: 200,
            body: Data(#"{"data":{"removeReaction":null},"errors":[{"type":"FORBIDDEN","message":"Resource not accessible by integration"}]}"#.utf8)
        ))
        let result = await reader(signIn: signIn).setReaction(
            .thumbsUp, add: false, on: "PR_kwDO42", host: GitHubFixtures.host, asLogin: "ana", source: source
        )
        #expect(result == .failure(.forbidden))
        #expect(transport.requests.map { $0.value(forHTTPHeaderField: "Authorization") } == ["Bearer ghu_a1"])
    }

    @Test func actionBoundToLentAccountSkipsDifferentOwnSignIn() async throws {
        let signInStore = GitHubSignInStore(vault: .memory())
        try signInStore.save(SignInFixtures.credential(login: "ana"))
        let signIn = GitHubSignIn(
            flow: GitHubDeviceFlow(transport: FakeTransport().transport, now: { GitHubFixtures.start }),
            store: signInStore
        )
        let bobLease = LeaseFetch.lease(LeaseAnswer(
            host: "github.com", login: "bob", token: GitHubToken("bob-token"), source: .gh
        ))
        lenders.answer("mac", bobLease)
        // detail() fails as ana (404), succeeds as bob -> viewerLogin == "bob".
        transport.enqueue(GitHubResponse(status: 404))
        transport.enqueue(try GitHubFixtures.response("pr_detail"))
        transport.enqueue(try PullRequestFilesTests.page(1))
        let target = PullRequestTarget(host: GitHubFixtures.host, owner: "acme", repo: "app", number: 42)
        let r = reader(signIn: signIn)
        let read = try #require(await r.detail(target, source: source))
        #expect(read.viewerLogin == "bob")

        transport.enqueue(try reactionResponse(add: false, groups: [
            ["content": "THUMBS_UP", "viewerHasReacted": false, "reactors": ["totalCount": 2]],
        ]))
        let toggled = await r.setReaction(
            .thumbsUp, add: false, on: read.detail.id, host: GitHubFixtures.host,
            asLogin: read.viewerLogin, source: source
        )
        #expect(toggled == .success([.init(content: .thumbsUp, count: 2, viewerHasReacted: false)]))
        // The mutation went directly to bob's token without touching ana's account.
        #expect(transport.requests.last?.value(forHTTPHeaderField: "Authorization") == "Bearer bob-token")
    }

    @Test func trackerRejectsStaleRefreshesAndPreservesConcurrentWrites() throws {
        var tracker = PullRequestReactionTracker()
        // Case 1: Refresh starts before a toggle, and finishes after the toggle already completed.
        let refreshBeforeToggle = tracker.beginRefresh()
        let first = tracker.beginToggle(.thumbsUp)
        let toggle1 = try #require(first)
        let duplicate = tracker.beginToggle(.thumbsUp)
        #expect(duplicate == nil)
        tracker.finishToggle(toggle1)
        #expect(tracker.pending.isEmpty)
        #expect(!tracker.acceptsRefresh(refreshBeforeToggle))

        // Case 2: Refresh starts while a toggle is in flight, and finishes after it completes.
        let second = tracker.beginToggle(.rocket)
        let toggle2 = try #require(second)
        let refreshDuringToggle = tracker.beginRefresh()
        tracker.finishToggle(toggle2)
        #expect(!tracker.acceptsRefresh(refreshDuringToggle))

        // Case 3: Refresh starts after all writes settled -> accepted.
        let freshRefresh = tracker.beginRefresh()
        #expect(tracker.acceptsRefresh(freshRefresh))

        // Case 4: Out-of-order concurrent writes for thumbsUp and rocket never overwrite each other.
        var shown: [PullRequestDetail.ReactionGroup] = [
            .init(content: .thumbsUp, count: 1, viewerHasReacted: true),
            .init(content: .rocket, count: 1, viewerHasReacted: true),
        ]
        // Rocket finishes first with server snapshot lacking thumbsUp.
        shown = PullRequestReactionTracker.apply(
            [.init(content: .rocket, count: 2, viewerHasReacted: true)],
            for: .rocket,
            into: shown
        )
        // ThumbsUp finishes second with server snapshot lacking rocket.
        shown = PullRequestReactionTracker.apply(
            [.init(content: .thumbsUp, count: 3, viewerHasReacted: true)],
            for: .thumbsUp,
            into: shown
        )
        #expect(shown == [
            .init(content: .thumbsUp, count: 3, viewerHasReacted: true),
            .init(content: .rocket, count: 2, viewerHasReacted: true),
        ])

        // Case 5: Stale toggle finishing after reset is no longer active and does not clear a newer toggle.
        let third = tracker.beginToggle(.heart)
        let staleToggle = try #require(third)
        #expect(tracker.isActive(staleToggle))
        tracker.reset()
        #expect(!tracker.isActive(staleToggle))
        let fourth = tracker.beginToggle(.heart)
        let freshToggle = try #require(fourth)
        #expect(!tracker.isActive(staleToggle))
        #expect(tracker.isActive(freshToggle))
        let staleFinished = tracker.finishToggle(staleToggle)
        #expect(!staleFinished)
        #expect(tracker.pending == [.heart])
        let freshFinished = tracker.finishToggle(freshToggle)
        #expect(freshFinished)
        #expect(tracker.pending.isEmpty)
    }
}
