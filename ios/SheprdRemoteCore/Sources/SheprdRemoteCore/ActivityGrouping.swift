import Foundation

/// The phone defaults to grouping by host. An explicit choice for a
/// particular host takes precedence over the phone-wide default.
public enum ActivityGrouping {
    public static let allHostsKey = "liveActivityAllHosts"

    public static func shared(for computer: Fingerprint, in defaults: UserDefaults = .standard) -> Bool {
        defaults.bool(forKey: allHostsKey) && defaults.object(forKey: key(for: computer)) == nil
    }

    public static let defaultKey = "multipleLiveActivities.default"

    public static func key(for computer: Fingerprint) -> String {
        "multipleLiveActivities." + computer.hex
    }

    /// A start can arrive after this phone ended all its activities. Per-pane
    /// cards have no generation. New shared cards carry the reset UUID.
    public static func isRetiredHostCard(_ pane: String, since epoch: UUID?) -> Bool {
        guard let epoch, pane.hasPrefix("host-activity/") else { return false }
        return !pane.hasPrefix("host-activity/" + epoch.uuidString.lowercased() + "/")
    }

    public static func single(for computer: Fingerprint, in defaults: UserDefaults = .standard) -> Bool {
        let override = defaults.object(forKey: key(for: computer)) as? Bool
        return !(override ?? (!defaults.bool(forKey: allHostsKey) && defaults.bool(forKey: defaultKey)))
    }
}
