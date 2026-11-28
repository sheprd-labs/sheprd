import Foundation
import Testing
@testable import SheprdGitHub

/// The Pull Requests tab's filter: what it reads from GitHub, and what it
/// keeps of what was read.
struct PullRequestFilterTests {
    private static func pull(_ number: UInt64, _ state: GitHubPullRequest.State, draft: Bool = false, repo: String = "sheprd-labs/sheprd") throws -> GitHubSearchPull {
        let host = try #require(GitHubHost("github.com"))
        let parts = repo.split(separator: "/").map(String.init)
        return GitHubSearchPull(host: host, owner: parts[0], repo: parts[1], number: number, title: "#\(number)",
                                state: state, isDraft: draft, headRefName: "b\(number)")
    }

    private static func lists() throws -> PullRequestLists {
        PullRequestLists(
            mine: [try pull(1, .open), try pull(2, .open, draft: true), try pull(3, .merged), try pull(4, .closed, repo: "Octo-Org/infra")],
            reviewRequested: [try pull(5, .open, repo: "sheprd-labs/sheprd-apns"), try pull(6, .merged)],
            mineTotal: 40, reviewRequestedTotal: 12, states: Set(PullRequestFilter.State.allCases)
        )
    }

    private static func variables(_ states: Set<PullRequestFilter.State>) throws -> [String: String] {
        let body = try JSONSerialization.jsonObject(with: PullRequestSearchQuery(states: states).body) as? [String: Any]
        return try #require(body?["variables"] as? [String: String])
    }

    @Test func openStatesAreOneSearchPerList() throws {
        let variables = try Self.variables([.open, .draft])
        #expect(variables.keys.sorted() == ["mine", "review"])
        #expect(variables["mine"]?.contains("is:open author:@me") == true)
    }

    @Test func eachStateSearchesForItselfAlone() throws {
        #expect(try Self.variables([.open])["mine"]?.contains("is:open draft:false") == true)
        #expect(try Self.variables([.draft])["review"]?.contains("is:open draft:true") == true)
        #expect(try Self.variables([.merged])["mineClosed"]?.contains("is:merged") == true)
        #expect(try Self.variables([.closed])["reviewClosed"]?.contains("is:closed is:unmerged") == true)
        #expect(try Self.variables([.merged]).keys.sorted() == ["mineClosed", "reviewClosed"])
    }

    @Test func closedOnesAreASecondSearchBesideTheOpenOnes() throws {
        let query = PullRequestSearchQuery(states: [.open, .draft, .merged, .closed])
        #expect(query.groups.map(\.qualifier) == ["is:open", "is:closed"])
        #expect(query.text.contains("mineClosed: search(type: ISSUE, query: $mineClosed"))
        #expect(!query.text.contains("@me"))
    }

    @Test func bothSearchesOfAListBecomeOneWithoutRepeats() throws {
        let node = { (number: Int, state: String) in
            #"{"number":\#(number),"title":"t","state":"\#(state)","isDraft":false,"headRefName":"b","repository":{"name":"r","owner":{"login":"o"}}}"#
        }
        let body = #"""
        {"data":{"mine":{"issueCount":2,"nodes":[\#(node(1, "OPEN")),\#(node(2, "OPEN"))]},"review":{"issueCount":0,"nodes":[]},
        "mineClosed":{"issueCount":30,"nodes":[\#(node(3, "MERGED")),\#(node(1, "OPEN"))]},"reviewClosed":{"issueCount":1,"nodes":[\#(node(4, "CLOSED"))]}}}
        """#
        let states: Set<PullRequestFilter.State> = [.open, .draft, .merged, .closed]
        let lists = try PullRequestSearchAnswer.parse(
            GitHubResponse(status: 200, body: Data(body.utf8)), host: GitHubFixtures.host, now: GitHubFixtures.start,
            query: PullRequestSearchQuery(states: states), states: states
        )
        #expect(lists.mine.map(\.number) == [1, 2, 3])
        #expect(lists.reviewRequested.map(\.number) == [4])
        #expect(lists.mineTotal == 32)
        #expect(lists.states == states)
    }

    @Test func aMissingClosedSearchFailsTheRead() throws {
        let body = #"{"data":{"mine":{"issueCount":0,"nodes":[]},"review":{"issueCount":0,"nodes":[]}}}"#
        #expect(throws: GitHubError.self) {
            try PullRequestSearchAnswer.parse(
                GitHubResponse(status: 200, body: Data(body.utf8)), host: GitHubFixtures.host, now: GitHubFixtures.start,
                query: PullRequestSearchQuery(states: [.open, .merged])
            )
        }
    }

    @Test func theStandardFilterKeepsTheOpenOnesAndTheirTotals() throws {
        let lists = try Self.lists()
        let filtered = lists.filtered(PullRequestFilter())
        #expect(filtered.mine.map(\.number) == [1, 2])
        #expect(filtered.reviewRequested.map(\.number) == [5])
        #expect(filtered.mineTotal == 2)
        #expect(PullRequestFilter().isStandard)
    }

    @Test func aFilterReadForItsStatesKeepsTheSearchTotals() throws {
        var lists = try Self.lists()
        lists.reviewRequested = [try Self.pull(6, .merged)]
        let filtered = lists.filtered(PullRequestFilter(states: [.merged]))
        #expect(filtered.reviewRequested.map(\.number) == [6])
        #expect(filtered.reviewRequestedTotal == 12)
    }

    @Test func statesSectionsAndRepositoriesNarrow() throws {
        let lists = try Self.lists()
        let closed = lists.filtered(PullRequestFilter(states: [.merged, .closed], showsReviewRequested: false))
        #expect(closed.mine.map(\.number) == [3, 4])
        #expect(closed.reviewRequested.isEmpty && closed.reviewRequestedTotal == 0)
        let octo = lists.filtered(PullRequestFilter(states: Set(PullRequestFilter.State.allCases), repository: "octo-org"))
        #expect(octo.mine.map(\.number) == [4])
        let apns = lists.filtered(PullRequestFilter(repository: "sheprd-labs/sheprd-apns"))
        #expect(apns.reviewRequested.map(\.number) == [5])
        #expect(apns.mine.isEmpty)
        // An owner is not a prefix of another owner's name.
        #expect(lists.filtered(PullRequestFilter(repository: "sheprd")).isEmpty)
    }

    @Test func noStateListsTheOpenOnes() {
        var filter = PullRequestFilter(states: [])
        #expect(filter.states == PullRequestFilter.openStates)
        filter.states = [.merged]
        filter.states.remove(.merged)
        #expect(filter.states == PullRequestFilter.openStates)
    }

    @Test func ownersGroupTheirRepositories() throws {
        let owners = try Self.lists().owners
        #expect(owners.map(\.owner) == ["octo-org", "sheprd-labs"])
        #expect(owners[1].repositories == ["sheprd-labs/sheprd", "sheprd-labs/sheprd-apns"])
    }

    @Test func theFilterIsKeptAsJSON() throws {
        let filter = PullRequestFilter(states: [.merged, .closed], showsMine: false, repository: "Octo-Org/Infra")
        #expect(PullRequestFilter(rawValue: filter.rawValue) == filter)
        #expect(filter.repository == "octo-org/infra")
        #expect(PullRequestFilter(rawValue: "{}") == PullRequestFilter())
        #expect(PullRequestFilter(rawValue: #"{"states":[]}"#)?.states == PullRequestFilter.openStates)
    }

    @Test func equalFiltersWriteTheSameText() throws {
        // `==` on a RawRepresentable compares raw values; a dictionary's
        // keys come out in any order unless sorted.
        for _ in 0..<20 {
            #expect(PullRequestFilter().rawValue == PullRequestFilter().rawValue)
            #expect(PullRequestFilter(rawValue: #"{"showsMine":true}"#)?.isStandard == true)
        }
        #expect(PullRequestFilter().rawValue == #"{"showsMine":true,"showsReviewRequested":true,"states":["draft","open"]}"#)
    }

    @Test func listsCachedBeforeTheFilterWereReadOpen() throws {
        let data = Data(#"{"mine":[],"reviewRequested":[],"mineTotal":0,"reviewRequestedTotal":0}"#.utf8)
        let lists = try JSONDecoder().decode(PullRequestLists.self, from: data)
        #expect(lists.states == PullRequestFilter.openStates)
    }
}
