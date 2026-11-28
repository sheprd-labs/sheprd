#if DEBUG
import Foundation
import MoonKit
import SheprdRemoteCore
import UIKit

/// Luau screens an agent on a paired computer changes while they run
/// (`sheprd phone luau` there): its files are laid over the screen's
/// bundled ones, in a folder of their own, and the screen reloads with its
/// state kept. A debug build only: an App Store build runs its bundled Luau
/// alone, as App Review guideline 2.5.2 asks (`tools/luau_store_audit.sh`).
@MainActor
final class LuauLive {
    static let shared = LuauLive()

    private final class Weak {
        weak var workspace: Workspace?
        init(_ workspace: Workspace) { self.workspace = workspace }
    }

    /// The screens open now, by name.
    private var open: [String: Weak] = [:]

    /// The bundled Luau of `screen`, as `Luau/pull-requests`.
    static func bundled(_ screen: String) -> URL? {
        Bundle.main.url(forResource: "Luau/\(screen)", withExtension: nil)
    }

    /// Where an agent's files for `screen` go, over the bundled ones.
    static func overlay(_ screen: String) -> URL {
        URL.applicationSupportDirectory.appending(path: "LuauLive/\(screen)")
    }

    /// The folders a screen loads from: the bundle's, with an agent's files
    /// over them.
    static func folders(_ screen: String, bundled: URL) -> Folders {
        Folders(builtin: bundled, user: overlay(screen))
    }

    /// `workspace` draws `screen`: a push reloads it.
    func register(_ workspace: Workspace, screen: String) {
        open[screen] = Weak(workspace)
    }

    /// Lays `files` over `screen`'s bundled ones (no files: back to the
    /// bundled ones), reloads it if it is open, else checks them without
    /// drawing, and says what that found.
    func apply(screen: String, files: [String: String]) -> AgentLuauReport {
        guard let bundled = Self.bundled(screen) else {
            return AgentLuauReport(shown: false, problems: ["this app has no Luau screen named \(screen)"])
        }
        do {
            try write(files, to: Self.overlay(screen))
        } catch {
            return AgentLuauReport(shown: false, problems: ["could not save the files: \(error.localizedDescription)"])
        }
        if let workspace = open[screen]?.workspace {
            workspace.reload()
            return Self.report(workspace, shown: true)
        }
        let workspace = Workspace(folders: Self.folders(screen, bundled: bundled))
        workspace.reload()
        return Self.report(workspace, shown: false)
    }

    /// Replaces the overlay with `files`: a file the push leaves out is the
    /// bundle's again. Paths were checked on the computer; checked again
    /// here, as nothing may land outside the folder.
    private func write(_ files: [String: String], to folder: URL) throws {
        let manager = FileManager.default
        try? manager.removeItem(at: folder)
        guard !files.isEmpty else { return }
        for (path, text) in files {
            let parts = path.split(separator: "/")
            guard path.hasSuffix(".luau"), !parts.isEmpty, parts.count <= 3,
                  parts.allSatisfy({ !$0.isEmpty && !$0.hasPrefix(".") })
            else { continue }
            let url = folder.appending(path: path)
            try manager.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(text.utf8).write(to: url, options: .atomic)
        }
    }

    private static func report(_ workspace: Workspace, shown: Bool) -> AgentLuauReport {
        // A Luau error names its file and line already.
        let files = workspace.problems.values.sorted { $0.file < $1.file }.map { problem in
            problem.message.hasPrefix(problem.file) ? problem.message : "\(problem.file): \(problem.message)"
        }
        let drawn = workspace.trees.sorted { $0.key < $1.key }.flatMap { mod, tree in
            tree.problems.map { "\(workspace.file(of: mod)): \($0)" }
        }
        return AgentLuauReport(shown: shown, problems: files + drawn, logs: workspace.logs)
    }

    /// A picture of the app's window as JPEG, for an agent to look at:
    /// what the person sees, the status bar left out.
    static func snapshot() -> Data? {
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first { $0.isKeyWindow }
        guard let window else { return nil }
        let format = UIGraphicsImageRendererFormat()
        format.scale = 2
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
        }
        return image.jpegData(compressionQuality: 0.8)
    }
}
#endif
