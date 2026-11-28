import UIKit
import XCTest

/// Settings › Developer switches the Pull Requests tab between SwiftUI and
/// Luau with no relaunch, and the Luau one searches and opens a pull
/// request's screen as the SwiftUI one does.
final class LuauPullRequestsTabTests: XCTestCase {
    @MainActor
    func testDeveloperSettingSwitchesTheTabLive() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Exercises the iPhone tab bar.")
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "YES", "-hideDemoBar", "YES", "-gitHubDemo", "YES", "-tab", "settings",
                               "-provisionalAlerts", "YES", "-AppleLanguages", "(en)"]
        app.launch()

        choose("Luau", in: app)
        app.buttons["PRs"].firstMatch.tap()
        let teleport = app.staticTexts["Teleport: carry checkpoints across hosts"]
        XCTAssertTrue(teleport.waitForExistence(timeout: 10))
        attach(app, "Luau PRs tab")

        teleport.tap()
        XCTAssertTrue(app.navigationBars["#1345"].waitForExistence(timeout: 5))
        app.navigationBars["#1345"].buttons.firstMatch.tap()

        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap()
        search.typeText("relay")
        XCTAssertTrue(app.staticTexts["Keep the relay's dial keys after a restart"].waitForExistence(timeout: 5))
        XCTAssertFalse(teleport.exists)
        app.buttons["Close"].firstMatch.tap()

        // Back to SwiftUI, live: the same rows.
        app.buttons["Settings"].firstMatch.tap()
        choose("SwiftUI", in: app)
        app.buttons["PRs"].firstMatch.tap()
        XCTAssertTrue(teleport.waitForExistence(timeout: 10))
    }

    @MainActor
    private func choose(_ engine: String, in app: XCUIApplication) {
        let row = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "PRs Tab")).firstMatch
        for _ in 0..<6 where !row.isHittable { app.swipeUp() }
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.tap()
        app.buttons[engine].firstMatch.tap()
        XCTAssertTrue(row.label.hasSuffix(engine == "Luau" ? "Luau" : "SwiftUI"))
        attach(app, "Settings › Developer: \(engine)")
    }

    @MainActor
    private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
