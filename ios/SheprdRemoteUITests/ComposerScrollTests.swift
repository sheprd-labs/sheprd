import XCTest

final class ComposerScrollTests: XCTestCase {
    @MainActor
    func testLastLineStaysVisibleWhenTypingPastHeightLimit() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "YES", "-demoChat", "c1", "-hideDemoBar", "YES",
                               "-provisionalAlerts", "YES", "-draft", ""]
        app.launch()

        let field = app.textViews.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.tap()
        let shortHeight = field.frame.height
        let wrappedLine = Array(repeating: "long message", count: 12).joined(separator: " ")
        let draft = wrappedLine + "\n2\n3\n4\n5"
        field.typeText(draft)
        let cappedHeight = field.frame.height
        XCTAssertGreaterThan(cappedHeight, shortHeight)
        field.typeText("\n6")
        XCTAssertEqual(field.frame.height, cappedHeight, accuracy: 1)

        // Tap past the text on the bottom visible line. If scrolling jumped
        // back, the next character lands on an earlier line of the draft.
        field.coordinate(withNormalizedOffset: CGVector(dx: 0.8, dy: 0.95)).tap()
        field.typeText("!")
        XCTAssertEqual(field.value as? String, draft + "\n6!")
    }
}
