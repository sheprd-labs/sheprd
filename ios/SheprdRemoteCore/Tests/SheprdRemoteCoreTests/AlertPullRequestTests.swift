import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// A finished turn's pull request: only a GitHub pull request page is
/// taken, and it adds Open PR beside Reply.
@Suite struct AlertPullRequestTests {
    static func finished(pr: String?, kind: String = "turn_ended") throws -> SealedAlert {
        var json: [String: Any] = ["kind": kind, "computer": "AB", "pane": "p", "title": "Claude finished", "body": "b"]
        json["pr"] = pr
        return try JSONDecoder().decode(SealedAlert.self, from: JSONSerialization.data(withJSONObject: json))
    }

    @Test func aGitHubPullRequestPageIsRead() throws {
        let pull = try #require(AlertPullRequest("https://github.com/sheprd-labs/sheprd/pull/1642"))
        #expect(pull.host == "github.com")
        #expect(pull.owner == "sheprd-labs")
        #expect(pull.repo == "sheprd")
        #expect(pull.number == 1642)
        #expect(pull.url.absoluteString == "https://github.com/sheprd-labs/sheprd/pull/1642")
        let enterprise = try #require(AlertPullRequest("https://github.example.com/o/r.js/pull/7"))
        #expect(enterprise.host == "github.example.com")
        #expect(AlertPullRequest("https://acme.ghe.com/o/r/pull/7") != nil)
    }

    @Test func anythingElseIsRefused() {
        for text in [
            "http://github.com/o/r/pull/7",
            "https://github.com/o/r/issues/7",
            "https://github.com/o/r/pull/7/files",
            "https://github.com/o/r/pull/7?x=1",
            "https://github.com/o/r/pull/7#top",
            "https://github.com/o/r/pull/07",
            "https://github.com/o/r/pull/0",
            "https://github.com/o/r/pull/",
            "https://github.com/o/r/pull/7/",
            "https://user@github.com/o/r/pull/7",
            "https://github.com:8443/o/r/pull/7",
            "https://GitHub.com/o/r/pull/7",
            "https://localhost/o/r/pull/7",
            "https://github.com/-o/r/pull/7",
            "https://github.com/o/../pull/7",
            "https://github.com/o%2Fx/r/pull/7",
            "javascript:alert(1)//github.com/o/r/pull/7",
            "https://github.com/o/r/pull/7" + String(repeating: "0", count: 600),
            "",
        ] {
            #expect(AlertPullRequest(text) == nil, "\(text)")
        }
    }

    @Test func aFinishedTurnWithOneOffersReplyAndOpenPR() throws {
        let presentation = try Self.finished(pr: "https://github.com/o/r/pull/7").presentation
        #expect(presentation.category == "finished.pr.7")
        #expect(presentation.actions.map(\.id) == ["reply", AlertPresentation.openPullRequestAction])
        #expect(presentation.actions.map(\.title) == ["Reply", "Open PR #7"])
        #expect(presentation.actions[1].style == .foreground)
        #expect(presentation.actions.allSatisfy { $0.icon != nil })
    }

    @Test func withoutAValidOneItIsJustFinished() throws {
        for pr in [nil, "https://example.com/o/r/issues/7"] {
            let alert = try Self.finished(pr: pr)
            #expect(alert.pullRequest == nil)
            #expect(alert.presentation == AlertPresentation.finished)
        }
        // Only a finished turn offers one.
        let error = try Self.finished(pr: "https://github.com/o/r/pull/7", kind: "error")
        #expect(error.pullRequest == nil)
        #expect(error.presentation.actions.isEmpty)
    }

    @Test func openPRSendsTheChatNothing() {
        #expect(AlertPresentation.command(action: AlertPresentation.openPullRequestAction, permission: nil, text: nil) == nil)
    }

    /// `Fixtures/notify/finished-pr.json`, written by
    /// `apns/tests/open_pr.rs`, opens to its pull request.
    @Test func theDaemonsFinishedPullRequestOpens() throws {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/notify/finished-pr", withExtension: "json"))
        let data = try Data(contentsOf: url)
        let fixture = try JSONDecoder().decode(SealedAlertFixtureTests.Fixture.self, from: data)
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        let alert = try SealedAlert.open(fixture.apns.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        let raw = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let plaintext = try #require(raw["plaintext"] as? [String: Any])
        #expect(alert.kind == .turnEnded)
        #expect(alert.pr == plaintext["pr"] as? String)
        #expect(alert.pullRequest?.number == 1642)
        #expect(alert.presentation.actions.map(\.title) == ["Reply", "Open PR #1642"])
    }
}
