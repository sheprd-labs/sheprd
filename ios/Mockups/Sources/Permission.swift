import SwiftUI
import SheprdRemoteCore

// A permission request whose title is a long shell command. State 0: the
// long command from a real session. State 1: a short one.
// A: a header and the command in a capped code card; Yes/No pinned.
// B: the command split into numbered steps; Yes/No pinned.
// C: a summary of what it does first, the command folded away.
// D: a two-line peek with Yes/No in reach; tap the peek for the full view.

private let longCommand = #"S=/Users/penso/.herdr/worktrees/herdr-gpui/worktree-green-forest-0c5b/target/release/Sheprd.app/Contents/MacOS/sheprd; cd /tmp/fleetvm && rm -f Sheprd2.zip && for f in chunk.*; do rm -f "$f"; done; ditto -c -k --keepParent /Users/penso/.herdr/worktrees/herdr-gpui/worktree-silver-cloud-24cf/target/release/Sheprd.app Sheprd2.zip; shasum Sheprd2.zip; base64 -i Sheprd2.zip | split -a 4 -b 190000 - chunk.; $S machine claim hoverfix >/dev/null; $S machine exec hoverfix -- /bin/sh -c 'cp /dev/null ~/fleet/Sheprd2.zip'; for c in chunk.*; do $S machine exec hoverfix -- /bin/sh -c 'printf %s "$1" | base64 -D >> ~/fleet/Sheprd2.zip' _ "$(cat $c)" >/dev/null || { echo fail; exit 1; }; done; $S machine exec hoverfix -- /bin/sh -c 'shasum ~/fleet/Sheprd2.zip'"#

private let shortCommand = "cargo test -p sheprd-remote-control"

private let blockedTint = Color(red: 0.92, green: 0.34, blue: 0.32)

private func command(_ state: Int) -> String { state == 1 ? shortCommand : longCommand }

/// Splits on `;`, `&&` and `||` outside quotes and braces, keeping `for …
/// done` loops whole, the way the real helper would.
private func steps(_ command: String) -> [String] {
    var out: [String] = []
    var current = ""
    var quote: Character?
    var depth = 0
    var chars = Array(command)
    var i = 0
    func flush() {
        let s = current.trimmingCharacters(in: .whitespaces)
        if !s.isEmpty { out.append(s) }
        current = ""
    }
    while i < chars.count {
        let c = chars[i]
        if let q = quote {
            if c == q { quote = nil }
            current.append(c); i += 1; continue
        }
        if c == "'" || c == "\"" { quote = c }
        if c == "{" { depth += 1 }
        if c == "}" { depth -= 1 }
        let rest = String(chars[i...].prefix(4))
        let word = current.trimmingCharacters(in: .whitespaces)
        if word.hasPrefix("for ") && !current.hasSuffix("done") {
            if rest.hasPrefix("done") { current += "done"; i += 4; continue }
            current.append(c); i += 1; continue
        }
        if depth == 0, c == ";" { flush(); i += 1; continue }
        if depth == 0, rest.hasPrefix("&&") || rest.hasPrefix("||") {
            flush(); i += 2; continue
        }
        current.append(c); i += 1
    }
    flush()
    chars = []
    return out
}

private struct PermissionBackdrop<Sheet: View>: View {
    @ViewBuilder var sheet: Sheet

    var body: some View {
        ChatDocument().sheet(isPresented: .constant(true)) { sheet }
    }
}

private struct RequestHeader: View {
    var subtitle: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "terminal.fill").font(.title3)
                .foregroundStyle(blockedTint)
                .frame(width: 40, height: 40)
                .background(blockedTint.opacity(0.15), in: .rect(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 2) {
                Text("Run this command?").font(.headline)
                Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
    }
}

private struct AnswerBar: View {
    var body: some View {
        VStack(spacing: 10) {
            HStack(spacing: 12) {
                Button {} label: { Text("No").frame(maxWidth: .infinity) }
                    .buttonStyle(.glass).controlSize(.large)
                Button {} label: { Text("Yes").frame(maxWidth: .infinity) }
                    .buttonStyle(.glassProminent).controlSize(.large).tint(blockedTint)
            }
            Button("Always allow in this folder") {}.font(.subheadline)
        }
        .padding(.horizontal, 24).padding(.top, 10).padding(.bottom, 8)
    }
}

private struct CodeCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .font(.system(.footnote, design: .monospaced))
            .textSelection(.enabled)
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.06), in: .rect(cornerRadius: 16))
            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.primary.opacity(0.08)))
    }
}

// MARK: A

/// A: a short header, the command in a monospaced card capped to ten
/// lines with a fade and "Show all"; Yes and No pinned side by side.
struct PermissionCodeCard: View {
    var state: Int

    var body: some View {
        PermissionBackdrop {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    RequestHeader(subtitle: "Claude · herdr-gpui")
                    let long = state != 1
                    CodeCard {
                        Text(command(state)).lineLimit(long ? 10 : nil)
                    }
                    .overlay(alignment: .bottom) {
                        if long {
                            HStack {
                                Spacer()
                                Label("Show all", systemImage: "chevron.down")
                                    .font(.subheadline.weight(.medium))
                                    .padding(.horizontal, 12).padding(.vertical, 6)
                                    .glassEffect(.regular, in: .capsule)
                            }
                            .padding(10)
                            .background(LinearGradient(colors: [.clear, Color(.systemBackground).opacity(0.9)],
                                                       startPoint: .top, endPoint: .bottom)
                                .clipShape(.rect(bottomLeadingRadius: 16, bottomTrailingRadius: 16)))
                        }
                    }
                    Label("Copy command", systemImage: "doc.on.doc").font(.subheadline)
                        .foregroundStyle(.tint)
                }
                .padding(24)
            }
            .safeAreaInset(edge: .bottom) { AnswerBar() }
            .presentationDetents([.medium, .large], selection: .constant(.large))
            .presentationDragIndicator(.visible)
        }
    }
}

// MARK: B

/// B: the command as numbered steps, one per `;`, `&&` or `||`, each in
/// monospace and wrapping; the first line names the step count.
struct PermissionSteps: View {
    var state: Int

    var body: some View {
        PermissionBackdrop {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    let parts = steps(command(state))
                    RequestHeader(subtitle: parts.count == 1 ? "1 step" : "\(parts.count) steps")
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(parts.enumerated()), id: \.offset) { index, step in
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Text("\(index + 1)").font(.caption.monospacedDigit().weight(.semibold))
                                    .foregroundStyle(.secondary).frame(width: 18, alignment: .trailing)
                                Text(step).font(.system(.footnote, design: .monospaced))
                                    .lineLimit(3)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .padding(.vertical, 9).padding(.horizontal, 12)
                            if index < parts.count - 1 { Divider().padding(.leading, 40) }
                        }
                    }
                    .background(Color.primary.opacity(0.06), in: .rect(cornerRadius: 16))
                }
                .padding(24)
            }
            .safeAreaInset(edge: .bottom) { AnswerBar() }
            .presentationDetents([.medium, .large], selection: .constant(.large))
            .presentationDragIndicator(.visible)
        }
    }
}

// MARK: C

/// C: what the command does, in words the phone can derive (folder, step
/// count, notable verbs flagged), and the command itself folded away.
struct PermissionSummary: View {
    var state: Int

    var body: some View {
        PermissionBackdrop {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    let long = state != 1
                    RequestHeader(subtitle: long ? "13 steps in /tmp/fleetvm" : "1 step in herdr-gpui")
                    if long {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("It will").font(.subheadline).foregroundStyle(.secondary)
                            FlowChips(chips: [
                                ("trash", "Delete files", true),
                                ("archivebox", "Zip Sheprd.app", false),
                                ("desktopcomputer", "Run on hoverfix", true),
                                ("arrow.up.doc", "Copy to ~/fleet", false),
                            ])
                        }
                    }
                    DisclosureGroup(isExpanded: .constant(!long)) {
                        CodeCard { Text(command(state)) }.padding(.top, 8)
                    } label: {
                        Text(long ? "Show the full command" : "Command").font(.subheadline.weight(.medium))
                    }
                    if long {
                        CodeCard { Text(String(command(state).prefix(140)) + " …").lineLimit(3) }
                    }
                }
                .padding(24)
            }
            .safeAreaInset(edge: .bottom) { AnswerBar() }
            .presentationDetents([.medium, .large], selection: .constant(.medium))
            .presentationDragIndicator(.visible)
        }
    }

    private struct FlowChips: View {
        var chips: [(String, String, Bool)]

        var body: some View {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(0 ..< (chips.count + 1) / 2, id: \.self) { row in
                    HStack(spacing: 8) {
                        ForEach(chips[(row * 2) ..< min(row * 2 + 2, chips.count)], id: \.1) { icon, text, warn in
                            Label(text, systemImage: icon).font(.subheadline.weight(.medium))
                                .padding(.horizontal, 10).padding(.vertical, 6)
                                .foregroundStyle(warn ? AnyShapeStyle(blockedTint) : AnyShapeStyle(.primary))
                                .background((warn ? blockedTint : Color.primary).opacity(0.12), in: .capsule)
                        }
                    }
                }
            }
        }
    }
}

// MARK: D

/// D: the sheet stays at half height: the header, a two-line peek of the
/// command and the answers, all in thumb reach. Tapping the peek (state 2)
/// opens the whole command full height, wrapped, with copy.
struct PermissionPeek: View {
    var state: Int

    var body: some View {
        PermissionBackdrop {
            if state == 2 {
                NavigationStack {
                    ScrollView {
                        Text(longCommand).font(.system(.callout, design: .monospaced))
                            .textSelection(.enabled)
                            .padding(20)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .navigationTitle("Command").navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .topBarLeading) { Button("Back", systemImage: "chevron.left") {} }
                        ToolbarItem(placement: .topBarTrailing) { Button("Copy", systemImage: "doc.on.doc") {} }
                    }
                    .safeAreaInset(edge: .bottom) { AnswerBar() }
                }
                .presentationDetents([.large])
            } else {
                VStack(alignment: .leading, spacing: 16) {
                    RequestHeader(subtitle: "Claude · herdr-gpui")
                    HStack(alignment: .center, spacing: 8) {
                        Text(command(state)).font(.system(.footnote, design: .monospaced)).lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if state != 1 {
                            Image(systemName: "arrow.up.left.and.arrow.down.right").foregroundStyle(.secondary)
                        }
                    }
                    .padding(14)
                    .background(Color.primary.opacity(0.06), in: .rect(cornerRadius: 16))
                    if state != 1 {
                        Text("Tap to read all 13 steps").font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                }
                .padding(24)
                .safeAreaInset(edge: .bottom) { AnswerBar() }
                .presentationDetents([.medium])
                .presentationDragIndicator(.visible)
            }
        }
    }
}

// MARK: Shipped

/// The app's own `PermissionRequest`, as D shipped. State 0: the long
/// command; state 1: a short command with more lines and an "always".
struct PermissionShipped: View {
    var state: Int

    var body: some View {
        PermissionBackdrop {
            PermissionRequest(permission: sample, answer: { _ in })
        }
    }

    /// Decoded as the phone receives it; the type has no public init.
    private var sample: Permission {
        let title = state == 1 ? "`\(shortCommand)` (+2 more lines)" : "`\(longCommand)`"
        let always = state == 1 ? #",{"id":"a","kind":"allow_always","name":"Always allow"}"# : ""
        let options = #"[{"id":"y","kind":"allow_once","name":"Yes"}"# + always + #",{"id":"n","kind":"reject_once","name":"No"}]"#
        let json = #"{"id":1,"title":"# + quoted(title) + #","options":"# + options + "}"
        // A fixed sample: decoding it cannot fail.
        return try! JSONDecoder().decode(Permission.self, from: Data(json.utf8))
    }

    private func quoted(_ text: String) -> String {
        String(decoding: (try? JSONEncoder().encode(text)) ?? Data(), as: UTF8.self)
    }
}
