import UIKit
import XCTest

final class HomeQuestionTests: XCTestCase {
    /// A question on Home opens its chat from its text, so the person can
    /// read what it refers to, while its options stay answer buttons.
    @MainActor
    func testQuestionTextOpensItsChat() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Home uses the agent list on iPhone.")
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-demo", "YES", "-hideDemoBar", "YES", "-provisionalAlerts", "YES", "-AppleLanguages", "(en)",
            "-collapsedHomeSections", "()", "-agentSort", "priority", "-agentScope", "<>",
            "-computerTagFilter", "", "-computerDeviceFilter", "", "-homeSearch", "", "-homeUnreadOnly", "NO"
        ]
        app.launch()

        XCTAssertTrue(app.buttons["Allow once"].waitForExistence(timeout: 10))
        let question = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS %@", "git push origin fix/widget-refresh"))
            .firstMatch
        XCTAssertTrue(question.waitForExistence(timeout: 5))
        question.tap()

        // The chat, which asks the question again in its own sheet.
        XCTAssertTrue(app.navigationBars.buttons["Back"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.navigationBars.staticTexts["Ship the widget fix"].waitForExistence(timeout: 5))
    }
}
