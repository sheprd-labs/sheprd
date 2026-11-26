// The effect views tap through the app's Feedback, which the mockups do not
// build; this stands in for it so they compile.
@MainActor
final class Feedback {
    static let shared = Feedback()
    enum Touch { case tap, select }
    func touch(_: Touch) {}
}

import SwiftUI
import SheprdRemoteCore

// The app's status tint, for the shipped permission sheet.
extension Home.AgentStatus {
    var color: Color { Color(red: 0.92, green: 0.34, blue: 0.32) }
}
