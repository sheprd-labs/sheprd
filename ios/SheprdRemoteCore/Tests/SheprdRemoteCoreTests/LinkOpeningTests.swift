import Foundation
import Testing
@testable import SheprdRemoteCore

struct LinkOpeningTests {
    private let web = BrowserNavigation.webSchemes

    private func url(_ link: String) -> URL {
        URL(string: link) ?? URL(fileURLWithPath: "/")
    }

    @Test func webLinksFollowTheSetting() {
        let link = url("https://github.com/sheprd-labs/sheprd/pull/1636")
        #expect(LinkOpening.sheprd.choice(for: link, schemes: web, othersToSystem: true) == .browse)
        #expect(LinkOpening.safari.choice(for: link, schemes: web, othersToSystem: true) == .system)
        #expect(LinkOpening.sheprd.choice(for: url("HTTP://example.com"), schemes: web, othersToSystem: false) == .browse)
    }

    @Test func otherSchemesGoToTheSystemOnlyWhenAllowed() {
        let mail = url("mailto:someone@example.com")
        #expect(LinkOpening.sheprd.choice(for: mail, schemes: web, othersToSystem: true) == .system)
        #expect(LinkOpening.sheprd.choice(for: mail, schemes: web, othersToSystem: false) == .drop)
        #expect(LinkOpening.safari.choice(for: mail, schemes: web, othersToSystem: false) == .drop)
    }

    @Test func aNarrowerSetDropsPlainHTTP() {
        let link = url("http://example.com")
        #expect(LinkOpening.sheprd.choice(for: link, schemes: ["https"], othersToSystem: false) == .drop)
        #expect(LinkOpening.safari.choice(for: link, schemes: ["https"], othersToSystem: false) == .drop)
    }
}
