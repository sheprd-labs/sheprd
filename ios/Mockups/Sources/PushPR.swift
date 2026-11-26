import SwiftUI

// A chat's "finished" notification when that turn opened or pushed to a
// pull request, drawn on a stand-in lock screen (the system draws real
// ones). State 0: the banner as it arrives. State 1: long-pressed, with
// its actions. A: the same text, one more action. B: a PR line in the
// body, the action names the number. C: the title says what happened to
// the PR, Open PR comes first. D: like A, with icons on the actions.

private struct PRSample {
    static let place = "herdr-gpui · Push open PR"
    static let answer = "Added the Open PR action to finished pushes; tests pass."
    static let worked = "Worked 7m 3s"
}

private struct NotificationCard: View {
    var title: String
    var subtitle: String
    var lines: [String]
    var expanded: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            RoundedRectangle(cornerRadius: 8)
                .fill(LinearGradient(colors: [.indigo, .purple], startPoint: .top, endPoint: .bottom))
                .frame(width: 38, height: 38)
                .overlay(Image(systemName: "pawprint.fill").foregroundStyle(.white).font(.system(size: 18)))
            VStack(alignment: .leading, spacing: 1) {
                HStack {
                    Text(title).font(.subheadline.weight(.semibold)).lineLimit(1)
                    Spacer()
                    Text("now").font(.caption).foregroundStyle(.secondary)
                }
                Text(subtitle).font(.subheadline.weight(.semibold)).lineLimit(1)
                ForEach(lines, id: \.self) { line in
                    Text(line).font(.subheadline).lineLimit(expanded ? nil : 2)
                }
            }
        }
        .padding(14)
        .background(.regularMaterial, in: .rect(cornerRadius: 22))
    }
}

private struct ActionMenu: View {
    var actions: [(String, String?)]

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(actions.enumerated()), id: \.offset) { index, action in
                if index > 0 { Divider() }
                HStack {
                    Text(action.0).font(.body)
                    Spacer()
                    if let icon = action.1 { Image(systemName: icon) }
                }
                .padding(.horizontal, 16).padding(.vertical, 12)
            }
        }
        .background(.regularMaterial, in: .rect(cornerRadius: 14))
        .frame(width: 250)
    }
}

private struct LockBackdrop<Content: View>: View {
    var expanded: Bool
    @ViewBuilder var content: Content

    var body: some View {
        ZStack(alignment: .top) {
            LinearGradient(colors: [Color(red: 0.12, green: 0.14, blue: 0.32), Color(red: 0.45, green: 0.2, blue: 0.4), Color(red: 0.9, green: 0.5, blue: 0.35)],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
                .blur(radius: expanded ? 20 : 0)
            VStack(spacing: 0) {
                if !expanded {
                    Text("Wednesday, November 25").font(.title3.weight(.semibold)).padding(.top, 70)
                    Text("9:41").font(.system(size: 96, weight: .semibold, design: .rounded))
                    Spacer()
                } else {
                    Spacer().frame(height: 160)
                }
                content.padding(.horizontal, 12)
                if expanded { Spacer() } else { Spacer().frame(height: 120) }
            }
            .foregroundStyle(.white)
        }
        .environment(\.colorScheme, .dark)
    }
}

struct PushPR: View {
    var variant: String
    var state: Int

    private var expanded: Bool { state == 1 }

    private var title: String {
        variant == "C" ? "Claude opened PR #1642" : "Claude finished"
    }

    private var lines: [String] {
        switch variant {
        case "B": [PRSample.answer, "Opened sheprd-labs/sheprd #1642 · \(PRSample.worked)"]
        default: [PRSample.answer, PRSample.worked]
        }
    }

    private var actions: [(String, String?)] {
        switch variant {
        case "B": [("Reply", nil), ("Open PR #1642", nil)]
        case "C": [("Open PR", nil), ("Reply", nil)]
        case "D": [("Reply", "arrowshape.turn.up.left"), ("Open PR #1642", "arrow.triangle.pull")]
        default: [("Reply", nil), ("Open PR", nil)]
        }
    }

    var body: some View {
        LockBackdrop(expanded: expanded) {
            VStack(alignment: .leading, spacing: 10) {
                NotificationCard(title: title, subtitle: PRSample.place, lines: lines, expanded: expanded)
                if expanded { ActionMenu(actions: actions) }
            }
        }
    }
}
