import Foundation
import Testing
@testable import SheprdRemoteCore

struct BrowserNavigationTests {
    private func decide(_ link: String, mainFrame: Bool = true, newWindow: Bool = false) -> BrowserNavigation {
        BrowserNavigation.decide(URL(string: link), mainFrame: mainFrame, newWindow: newWindow)
    }

    @Test func webPagesLoadWhereTheyWereHeaded() {
        #expect(decide("https://github.com/sheprd-labs/sheprd/pull/1457") == .allow)
        #expect(decide("HTTP://example.com") == .allow)
        #expect(decide("https://example.com", mainFrame: false) == .allow)
    }

    @Test func aNewWindowLoadsInTheOnePage() {
        #expect(decide("https://example.com", newWindow: true) == .loadHere)
        #expect(decide("about:blank", mainFrame: false, newWindow: true) == .refuse)
    }

    @Test func framesMayLoadBlankAndGeneratedDocuments() {
        #expect(decide("about:blank", mainFrame: false) == .allow)
        #expect(decide("data:text/html,hi", mainFrame: false) == .allow)
        #expect(decide("about:blank") == .refuse)
    }

    @Test func otherSchemesAreRefused() {
        for link in ["herdr-remote://pair?x=1", "tel:123", "mailto:a@example.com", "file:///etc/hosts", "javascript:alert(1)"] {
            #expect(decide(link) == .refuse, "\(link)")
        }
        #expect(BrowserNavigation.decide(nil, mainFrame: true, newWindow: false) == .refuse)
    }
}

struct BrowserFailureTests {
    @Test func aLoadThatCouldNotReachItsHostIsReported() {
        #expect(BrowserNavigation.reports(NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet)))
        #expect(BrowserNavigation.reports(NSError(domain: NSURLErrorDomain, code: NSURLErrorCannotFindHost)))
    }

    @Test func aLoadReplacedOrRefusedOnPurposeIsNot() {
        #expect(!BrowserNavigation.reports(NSError(domain: NSURLErrorDomain, code: NSURLErrorCancelled)))
        #expect(!BrowserNavigation.reports(NSError(domain: "WebKitErrorDomain", code: 102)))
    }
}
