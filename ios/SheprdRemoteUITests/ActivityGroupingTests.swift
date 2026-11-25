import UIKit
import XCTest

final class ActivityGroupingTests: XCTestCase {
    @MainActor
    func testHostsCanFollowOrOverrideTheDefaultGrouping() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Exercises the settings flow on iPhone.")
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "YES", "-hideDemoBar", "YES", "-tab", "settings",
                               "-openNotifications", "YES", "-provisionalAlerts", "YES", "-AppleLanguages", "(en)"]
        app.launch()
        func row(_ label: String) -> XCUIElement {
            app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", label + ", ")).firstMatch
        }
        func choose(_ option: String, on row: XCUIElement) {
            row.tap()
            app.buttons[option].tap()
        }
        let grouping = row("Default grouping")
        let studio = row("Studio")
        let laptop = row("MacBook Air")
        XCTAssertTrue(grouping.waitForExistence(timeout: 10))
        choose("One per agent", on: grouping)
        choose("One per host", on: studio)
        choose("Use default", on: laptop)
        app.terminate()
        app.launch()
        XCTAssertTrue(grouping.waitForExistence(timeout: 10))
        XCTAssertTrue(grouping.label.hasSuffix("One per agent"))
        XCTAssertTrue(studio.label.hasSuffix("One per host"))
        XCTAssertTrue(laptop.label.hasSuffix("Use default"))
        choose("One for all hosts", on: grouping)
        XCTAssertTrue(grouping.label.hasSuffix("One for all hosts"))
        XCTAssertTrue(studio.label.hasSuffix("One per host"))
        XCTAssertTrue(laptop.label.hasSuffix("Use default"))
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "One activity for all hosts with a per-host override"
        shot.lifetime = .keepAlways
        add(shot)
        choose("Use default", on: studio)
        choose("One per host", on: grouping)
        XCTAssertTrue(studio.label.hasSuffix("Use default"))
    }
}
