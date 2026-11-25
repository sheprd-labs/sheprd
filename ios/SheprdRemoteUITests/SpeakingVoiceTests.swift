import XCTest

final class SpeakingVoiceTests: XCTestCase {
    @MainActor
    func testKeyboardCommandOpensWithoutMicrophoneAndReachesReview() {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "YES", "-hideDemoBar", "YES", "-voiceKeyboardInput", "YES",
                               "-voiceSpeaks", "NO", "-provisionalAlerts", "YES", "-AppleLanguages", "(en)"]
        app.launch()
        app.buttons["Talk to Sheprd"].tap()
        let input = app.descendants(matching: .any).matching(identifier: "voice-keyboard-command").firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("What needs me?")
        app.buttons["Review Command"].tap()
        XCTAssertTrue(input.waitForNonExistence(timeout: 8))
        XCTAssertFalse(app.buttons["Stop listening"].exists)
    }

    @MainActor
    func testProvidersAndKeyboardDictation() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "YES", "-tab", "settings", "-hideDemoBar", "YES", "-provisionalAlerts", "YES", "-AppleLanguages", "(en)"]
        app.launch()
        app.buttons["Voice"].tap()
        let picker = app.buttons["speech-provider"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        picker.tap()
        for option in ["apple", "elevenlabs", "openai", "deepgram", "openrouter"] {
            XCTAssertTrue(app.buttons["voice-option-\(option)"].waitForExistence(timeout: 3))
        }
        XCTAssertTrue(app.buttons["voice-option-apple"].label.contains("In use"))
        let photo = XCTAttachment(screenshot: app.screenshot())
        photo.name = "Provider sheet"
        photo.lifetime = .keepAlways
        add(photo)
        app.buttons["close-provider-list"].tap()
        XCTAssertTrue(picker.label.contains("Apple"))
        picker.tap()
        app.buttons["voice-option-openrouter"].tap()
        XCTAssertTrue(app.buttons["Sign in with OpenRouter"].exists)
        let key = app.secureTextFields["API key"]
        XCTAssertTrue(key.exists)
        key.tap()
        key.typeText("local-ui-test-key")
        app.buttons["Use This Voice"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertTrue(picker.label.contains("OpenRouter"))
        app.terminate()
        app.launch()
        app.buttons["Voice"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertTrue(picker.label.contains("OpenRouter"))
        picker.tap()
        XCTAssertTrue(app.buttons["voice-option-openrouter"].label.contains("In use"))
        app.buttons["voice-option-openrouter"].tap()
        app.buttons["Disconnect"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertTrue(picker.label.contains("Apple"))
        let keyboard = app.switches["Use Keyboard for Voice Commands"]
        for _ in 0..<3 where !keyboard.isHittable { app.swipeUp() }
        XCTAssertTrue(keyboard.exists)
    }
}
