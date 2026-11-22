import SheprdRemoteCore
import SwiftUI

/// Room to write or paste long text before it goes to the terminal, where
/// each key would otherwise go as typed. Pasting leaves Return to the
/// person, as dictation does, so nothing runs until they say. The draft
/// stays until the terminal has all of it, so a failed paste loses nothing.
struct TermWriteSheet: View {
    @Environment(\.dismiss) private var dismiss
    /// Kept by the terminal, so a cancelled draft is there next time.
    @Binding var draft: String
    /// Pastes the text and calls back whether all of it arrived.
    let paste: (String, @escaping (Bool) -> Void) -> Void
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            TextEditor(text: $draft)
                .font(.system(.body, design: .monospaced))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .focused($focused)
                .padding(.horizontal, 8)
                .overlay(alignment: .topLeading) {
                    if draft.isEmpty {
                        Text("Write or paste text for the terminal")
                            .font(.system(.body, design: .monospaced))
                            .foregroundStyle(.tertiary)
                            .padding(.horizontal, 13)
                            .padding(.vertical, 8)
                            .allowsHitTesting(false)
                    }
                }
                .navigationTitle("Write Text")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel", role: .cancel) { dismiss() }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Paste", systemImage: "arrow.up") {
                            let sent = draft
                            paste(sent) { delivered in
                                // Edited since: what they wrote now stays.
                                if delivered, draft == sent { draft = "" }
                            }
                            dismiss()
                        }
                        .disabled(draft.isEmpty || tooLong)
                    }
                    ToolbarItem(placement: .bottomBar) {
                        Text(summary).font(.footnote).foregroundStyle(tooLong ? .orange : .secondary)
                    }
                }
                .onAppear { focused = true }
        }
    }

    private var tooLong: Bool { draft.utf8.count > TermInput.maxPasted }

    private var summary: String {
        let bytes = draft.utf8.count
        guard !tooLong else { return "Too long to paste; the limit is 1 MB." }
        let lines = draft.isEmpty ? 0 : draft.split(separator: "\n", omittingEmptySubsequences: false).count
        let size = ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
        return "\(lines) \(lines == 1 ? "line" : "lines"), \(size)"
    }
}

/// A file chosen in Files, read for upload; bounded before it is read, as
/// the computer takes at most 16 MB.
struct TermFile: Sendable {
    static let maxBytes = 16 * 1024 * 1024

    enum Failure: LocalizedError {
        case tooLarge(String)
        case unreadable(String)

        var errorDescription: String? {
            switch self {
            case let .tooLarge(name): "\(name) is larger than 16 MB."
            case let .unreadable(name): "\(name) could not be read."
            }
        }
    }

    let name: String
    let data: Data

    /// Reads `url` off the main thread, inside its security scope.
    static func read(_ url: URL) async throws -> TermFile {
        try await Task.detached(priority: .userInitiated) {
            let name = url.lastPathComponent
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
            guard size <= maxBytes else { throw Failure.tooLarge(name) }
            guard let data = try? Data(contentsOf: url, options: .mappedIfSafe) else { throw Failure.unreadable(name) }
            guard data.count <= maxBytes else { throw Failure.tooLarge(name) }
            return TermFile(name: name, data: data)
        }.value
    }
}
