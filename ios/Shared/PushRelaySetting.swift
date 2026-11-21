import Foundation

/// Whether this phone uses the Sheprd push relay (`docs/apns-relay-plan.md`),
/// off until the user turns it on in Settings. Kept in the App Group's
/// defaults, so the widget extension knows to show no Live Activity text
/// that came unsealed while the push relay holds this phone's tokens.
enum PushRelaySetting {
    static let key = "pushRelay"
    static var store: UserDefaults? { UserDefaults(suiteName: NotifyKey.group) }

    static var isOn: Bool {
        store?.bool(forKey: key) ?? false
    }
}
