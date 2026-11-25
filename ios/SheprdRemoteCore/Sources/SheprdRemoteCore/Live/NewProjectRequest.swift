import Foundation

/// A folder to initialize or clone under a root offered by the selected computer.
public struct NewProjectRequest: Encodable, Equatable, Sendable {
    public let root: String
    public let name: String
    public let clone: String?

    public init?(root: String, name: String, clone: String?) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let clone = clone?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !root.isEmpty, !name.isEmpty, name.count <= 100,
              !name.hasPrefix("."), !name.hasPrefix("-"),
              name.rangeOfCharacter(from: CharacterSet(charactersIn: "/\\:").union(.controlCharacters)) == nil,
              clone?.isEmpty != true else { return nil }
        self.root = root
        self.name = name
        self.clone = clone
    }
}
