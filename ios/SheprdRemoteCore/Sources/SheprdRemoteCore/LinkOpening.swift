import Foundation

/// Where the app opens web links: in its own browser sheet, or in Safari.
public enum LinkOpening: String, CaseIterable, Identifiable, Sendable {
    case sheprd, safari

    /// Where the choice is kept.
    public static let key = "linkOpening"

    public var id: Self { self }

    public var title: String {
        switch self {
        case .sheprd: "Sheprd"
        case .safari: "Safari"
        }
    }

    /// What to do with a tapped link.
    public enum Choice: Equatable, Sendable {
        /// Show it in the in-app browser.
        case browse
        /// Hand it to the system: Safari for a web page, else its app.
        case system
        /// Drop it.
        case drop
    }

    /// How to open `url`: a link whose scheme is in `schemes` follows this
    /// setting; any other goes to the system when `othersToSystem`, else is
    /// dropped, as links in untrusted text are.
    public func choice(for url: URL, schemes: Set<String>, othersToSystem: Bool) -> Choice {
        guard schemes.contains(url.scheme?.lowercased() ?? "") else { return othersToSystem ? .system : .drop }
        return self == .sheprd ? .browse : .system
    }
}
