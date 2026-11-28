import Foundation
import Testing
@testable import SheprdGitHub
@testable import SheprdRemoteCore

/// A repository's open pull requests and issues, for a new workspace.
struct RepositoryItemsTests {
    private func parse(_ json: String) throws(GitHubError) -> RepositoryItems {
        try RepositoryItemsQuery.parse(GitHubResponse(status: 200, body: Data(json.utf8)), now: GitHubFixtures.start)
    }

    @Test func theRepositoryTravelsAsVariables() throws {
        let body = try #require(try JSONSerialization.jsonObject(with: RepositoryItemsQuery.body(owner: "acme", repo: "app")) as? [String: Any])
        let variables = try #require(body["variables"] as? [String: Any])
        #expect(variables["owner"] as? String == "acme")
        #expect(variables["repo"] as? String == "app")
        #expect(variables["count"] as? Int == RepositoryItemsQuery.limit)
        #expect(!(try #require(body["query"] as? String)).contains("acme"))
    }

    @Test func pullRequestsAndIssuesAreRead() throws {
        let items = try parse("""
        {"data":{"repository":{
          "pullRequests":{"nodes":[
            {"number":12,"title":"Add\\u202e things","isDraft":true,"headRefName":"feature/x","author":{"login":"ana"},"headRepositoryOwner":{"login":"ana"}},
            {"number":13,"title":"No head","headRefName":"bad name","author":{"login":"bo"}},
            null
          ]},
          "issues":{"nodes":[{"number":5,"title":"Crash on start!","author":{"login":"cy"}}]}
        }}}
        """)
        #expect(items.pulls == [RepositoryItem(kind: .pull, number: 12, title: "Add things", author: "ana", isDraft: true, head: "feature/x", headOwner: "ana")])
        #expect(items.issues == [RepositoryItem(kind: .issue, number: 5, title: "Crash on start!", author: "cy")])
    }

    @Test func aRepositoryTheTokenCannotSeeIsNotFound() {
        #expect(throws: GitHubError.notFound) { try parse(#"{"data":{"repository":null}}"#) }
        #expect(throws: GitHubError.notFound) {
            try parse(#"{"data":{"repository":null},"errors":[{"type":"NOT_FOUND"}]}"#)
        }
    }

    @Test func anIssueBranchIsNamedAsTheDesktopNamesIt() {
        #expect(RepositoryItem(kind: .issue, number: 5, title: "Crash on start!").issueBranch == "5-crash-on-start")
        #expect(RepositoryItem(kind: .issue, number: 6, title: "¡¿").issueBranch == "6")
        let long = RepositoryItem(kind: .issue, number: 7, title: String(repeating: "ab ", count: 40)).issueBranch
        #expect(long.count <= "7-".count + 60)
        #expect(!long.hasSuffix("-"))
    }

    @Test func aSearchFindsNumberTitleAuthorAndBranch() {
        let pull = RepositoryItem(kind: .pull, number: 42, title: "Tidy menus", author: "ana", head: "tidy/menus")
        #expect(pull.matches("#42"))
        #expect(pull.matches("TIDY"))
        #expect(pull.matches("ana"))
        #expect(pull.matches("tidy/"))
        #expect(!pull.matches("zebra"))
        #expect(pull.matches("  "))
    }
}
