import Foundation

/// The pull request a finished turn created or pushed to, which its
/// notification offers to open (`SealedAlert.pr`): only a GitHub pull
/// request's own page, `https://HOST/OWNER/REPO/pull/NUMBER`, on github.com
/// or a GitHub Enterprise host, with nothing after the number. The daemon
/// writes it the same way (`PullRequest::url_of`).
public struct AlertPullRequest: Hashable, Sendable {
    /// The host's name in lowercase, such as `github.com`.
    public let host: String
    public let owner: String
    public let repo: String
    public let number: UInt64
    public let url: URL

    /// The longest address taken, in bytes.
    static let maxLength = 512

    public init?(_ text: String) {
        guard text.utf8.count <= Self.maxLength,
              let parts = URLComponents(string: text),
              parts.scheme == "https",
              parts.user == nil, parts.password == nil, parts.port == nil,
              parts.query == nil, parts.fragment == nil,
              let host = parts.host, Self.validHost(host)
        else { return nil }
        let path = parts.path.split(separator: "/", omittingEmptySubsequences: false)
        guard path.count == 5, path[0].isEmpty, path[3] == "pull",
              Self.validOwner(path[1]), Self.validRepo(path[2]),
              let number = UInt64(path[4]), number > 0, String(number) == path[4],
              let url = URL(string: text)
        else { return nil }
        self.host = host
        owner = String(path[1])
        repo = String(path[2])
        self.number = number
        self.url = url
    }

    /// A lowercase DNS name with at least one dot: letters, digits, `-`,
    /// and `.`, no label empty or edged with `-`.
    static func validHost(_ host: String) -> Bool {
        let labels = host.split(separator: ".", omittingEmptySubsequences: false)
        return host.utf8.count <= 253 && labels.count >= 2 && labels.allSatisfy { label in
            !label.isEmpty && label.count <= 63 && !label.hasPrefix("-") && !label.hasSuffix("-")
                && label.unicodeScalars.allSatisfy { ("a"..."z").contains($0) || ("0"..."9").contains($0) || $0 == "-" }
        }
    }

    /// A GitHub account: letters, digits, and inner hyphens, as the daemon
    /// checks.
    static func validOwner(_ owner: Substring) -> Bool {
        !owner.isEmpty && owner.count <= 39 && !owner.hasPrefix("-") && !owner.hasSuffix("-")
            && owner.unicodeScalars.allSatisfy { $0.isASCII && ($0.properties.isAlphabetic || ("0"..."9").contains($0) || $0 == "-") }
    }

    /// A GitHub repository: letters, digits, `.`, `-`, and `_`, but not a
    /// name that reads as a directory.
    static func validRepo(_ repo: Substring) -> Bool {
        !repo.isEmpty && repo.count <= 100 && repo != "." && repo != ".."
            && repo.unicodeScalars.allSatisfy { $0.isASCII && ($0.properties.isAlphabetic || ("0"..."9").contains($0) || "._-".unicodeScalars.contains($0)) }
    }
}
