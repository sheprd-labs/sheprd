import Foundation

/// What the in-app browser does with a navigation. Links come from
/// untrusted text (agents' answers, GitHub comments), so the browser only
/// shows web pages: anything else, such as another app's URL scheme, is
/// refused rather than handed to the system.
public enum BrowserNavigation: Equatable, Sendable {
    /// Load it where it was headed.
    case allow
    /// A link meant for a new window: the browser has one page, which
    /// loads it instead, so its history stacks up behind the back button.
    case loadHere
    /// Not a web page.
    case refuse

    /// The schemes a page may show at the top level.
    public static let webSchemes: Set<String> = ["http", "https"]
    /// What a page's own frames load beside the web: blank and generated
    /// documents.
    static let frameSchemes: Set<String> = ["about", "blob", "data"]

    /// How to treat `url`: headed for the main frame or one of its frames,
    /// or for a new window (no target frame).
    public static func decide(_ url: URL?, mainFrame: Bool, newWindow: Bool) -> Self {
        let scheme = url?.scheme?.lowercased() ?? ""
        if webSchemes.contains(scheme) { return newWindow ? .loadHere : .allow }
        if !mainFrame, !newWindow, frameSchemes.contains(scheme) { return .allow }
        return .refuse
    }
}

extension BrowserNavigation {
    /// Whether a load's failure is worth telling the reader. A load cut
    /// short on purpose is not: another one replaced it (a tapped link, a
    /// new window loaded here) or the decider refused it.
    public static func reports(_ error: NSError) -> Bool {
        let cancelled = error.domain == NSURLErrorDomain && error.code == NSURLErrorCancelled
        // WebKitErrorFrameLoadInterruptedByPolicyChange, which WebKit's
        // headers declare but Foundation does not.
        let interrupted = error.domain == "WebKitErrorDomain" && error.code == 102
        return !cancelled && !interrupted
    }
}
