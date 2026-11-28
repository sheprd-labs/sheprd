import XCTest

final class PullRequestFilterTests: XCTestCase {
    @MainActor
    func testTheFilterListsMergedPullRequests() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-tab", "pullRequests", "-gitHubDemo", "signedIn", "-provisionalAlerts", "YES", "-AppleLanguages", "(en)"]
        app.launch()
        let merged = app.staticTexts["Docs: pairing with a QR code"]
        let filter = app.buttons["Filter pull requests"]
        // A run before may have left it filtered.
        if !filter.waitForExistence(timeout: 10) {
            app.buttons["Filter pull requests, filtered"].tap()
            app.buttons["Show Open Pull Requests"].tap()
        }
        XCTAssertTrue(filter.waitForExistence(timeout: 5))
        XCTAssertFalse(merged.exists)

        filter.tap()
        let mergedToggle = app.buttons["Merged"]
        XCTAssertTrue(mergedToggle.waitForExistence(timeout: 5))
        mergedToggle.tap()
        // The menu stays open while states are ticked.
        XCTAssertTrue(app.buttons["Closed"].isHittable)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "Filter menu with Merged ticked"
        shot.lifetime = .keepAlways
        add(shot)
        app.buttons["Open"].tap()
        app.buttons["Draft"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95)).tap()

        XCTAssertTrue(merged.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Keep the relay's dial keys after a restart"].exists)
        XCTAssertTrue(app.buttons["Filter pull requests, filtered"].exists)
        let filtered = XCTAttachment(screenshot: app.screenshot())
        filtered.name = "Merged pull requests only"
        filtered.lifetime = .keepAlways
        add(filtered)

        app.buttons["Filter pull requests, filtered"].tap()
        app.buttons["Show Open Pull Requests"].tap()
        XCTAssertTrue(filter.waitForExistence(timeout: 5))
    }
}
