import UIKit
import XCTest

final class HomeUnreadTests: XCTestCase {
    @MainActor
    func testToolbarFiltersAndRestoresDemoAgents() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Home uses the agent list on iPhone.")
        continueAfterFailure = false
        let app = XCUIApplication()
        // Override list preferences for this launch, including any folded
        // demo sections. The unread-only setting must stay writable.
        app.launchArguments = [
            "-demo", "YES", "-hideDemoBar", "YES", "-provisionalAlerts", "YES", "-AppleLanguages", "(en)",
            "-collapsedHomeSections", "()", "-agentSort", "priority", "-agentScope", "<>",
            "-computerTagFilter", "", "-computerDeviceFilter", "", "-homeSearch", ""
        ]
        app.launch()

        let unread = app.navigationBars.buttons["Unread 1"]
        XCTAssertTrue(unread.waitForExistence(timeout: 10))
        let readAgent = app.staticTexts["Ship the widget fix"]
        if unread.value as? String == "Unread Only" {
            unread.tap()
            app.buttons["Unread Only"].tap()
        }
        XCTAssertTrue(readAgent.waitForExistence(timeout: 5))
        XCTAssertEqual(unread.value as? String, "All Agents")
        XCTAssertFalse(app.buttons["Mark All Read"].exists)

        unread.tap()
        XCTAssertTrue(app.buttons["Mark All Read"].exists)
        app.buttons["Unread Only"].tap()
        XCTAssertTrue(readAgent.waitForNonExistence(timeout: 5))
        XCTAssertEqual(unread.value as? String, "Unread Only")
        let lastUnread = app.staticTexts["Weather widget"]
        XCTAssertTrue(lastUnread.exists)

        lastUnread.tap()
        app.navigationBars.buttons["Back"].tap()
        let zero = app.navigationBars.buttons["Unread 0"]
        XCTAssertTrue(zero.waitForExistence(timeout: 5))
        XCTAssertFalse(lastUnread.exists)
        zero.tap()
        XCTAssertFalse(app.buttons["Mark All Read"].isEnabled)
        app.buttons["Unread Only"].tap()
        XCTAssertTrue(readAgent.waitForExistence(timeout: 5))
        XCTAssertTrue(lastUnread.exists)
        XCTAssertFalse(zero.exists)
    }
}
