import Foundation

/// Pictures this app sent into a terminal agent's prompt. The computer
/// stores each and types its path, so the agent's transcript has the path
/// where the picture was; a heading shows a picture mark for it instead.
public enum PromptPictures {
    private static let idAlphabet = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_")

    /// `text` without the paths of pictures this app uploaded, and how many
    /// there were. Any other path, such as one the person typed, stays.
    public static func split(_ text: String) -> (words: String, pictures: Int) {
        var count = 0
        let kept = text.split(separator: " ", omittingEmptySubsequences: false).filter { word in
            let upload = isUpload(word)
            if upload { count += 1 }
            return !upload
        }
        return (kept.joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines), count)
    }

    /// An absolute path ending in `/uploads/`, a random 22-character id,
    /// and `.jpg`: where the computer stores a picture from this app.
    public static func isUpload(_ word: Substring) -> Bool {
        guard word.hasPrefix("/"), word.hasSuffix(".jpg"), let slash = word.lastIndex(of: "/") else { return false }
        let id = word[word.index(after: slash)...].dropLast(4)
        return word[..<slash].hasSuffix("/uploads")
            && id.count == 22
            && id.unicodeScalars.allSatisfy(idAlphabet.contains)
    }
}
