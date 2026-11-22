import SheprdRemoteCore
import SwiftUI

/// What a row's read mark names: an agent's pane on its computer, which
/// keeps what every phone and Sheprd read, and whether it is unread now.
struct ReadTarget: Hashable {
    let key: PaneKey
    let unread: Bool
}

extension HomeRow {
    /// The agent the row's read mark names; none for a terminal agent an
    /// older computer does not name the pane of.
    var readTarget: ReadTarget? {
        switch self {
        case let .chat(item):
            ReadTarget(key: item.key, unread: item.unread)
        case let .agent(item):
            item.agent?.pane.map {
                ReadTarget(key: PaneKey(device: item.computer.device, pane: $0), unread: item.unread)
            }
        }
    }

    /// Whether the agent has a reply no one read.
    var unread: Bool {
        switch self {
        case let .chat(item): item.unread
        case let .agent(item): item.unread
        }
    }
}

extension AppModel {
    /// Marks `keys` read or unread on their computers. Rows change when each
    /// computer's next home says so, on every device; a refusal is told.
    func setRead(_ read: Bool, _ keys: [PaneKey]) {
        guard !keys.isEmpty else { return }
        Task {
            do {
                try await fleet.setRead(read, for: keys)
            } catch {
                noticeTitle = read ? String(localized: "Not marked as read") : String(localized: "Not marked as unread")
                notice = error.localizedDescription
            }
        }
    }
}

/// Read or Unread at a row's leading edge, as Mail and Messages have it.
struct ReadSwipeButton: View {
    @Environment(AppModel.self) private var model
    let target: ReadTarget

    var body: some View {
        Button { model.setRead(target.unread, [target.key]) } label: {
            target.unread
                ? Label("Read", systemImage: "envelope.open.fill")
                : Label("Unread", systemImage: "envelope.badge.fill")
        }
        .tint(target.unread ? .gray : .blue)
    }
}

/// Read or Unread in a row's context menu.
struct ReadMenuButton: View {
    @Environment(AppModel.self) private var model
    let target: ReadTarget

    var body: some View {
        if target.unread {
            Button("Mark as Read", systemImage: "envelope.open") { model.setRead(true, [target.key]) }
        } else {
            Button("Mark as Unread", systemImage: "envelope.badge") { model.setRead(false, [target.key]) }
        }
    }
}

/// The dot before an unread row, where Mail puts it.
struct UnreadDot: View {
    var unread: Bool

    var body: some View {
        Circle()
            .fill(.tint)
            .frame(width: 10, height: 10)
            .opacity(unread ? 1 : 0)
            .accessibilityHidden(!unread)
            .accessibilityLabel("Unread")
    }
}

/// Above Home's list while any agent is unread: shows only those, or
/// marks them all read.
struct UnreadBar: View {
    @Environment(AppModel.self) private var model
    /// The unread agents among those the filters leave.
    var unread: [PaneKey]
    @Binding var only: Bool

    var body: some View {
        HStack(spacing: 8) {
            Button {
                withAnimation(.snappy) { only.toggle() }
            } label: {
                Text("Unread \(unread.count)")
                    .font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 12).padding(.vertical, 6)
                    .foregroundStyle(only ? Color.white : Color.primary)
                    .background(only ? AnyShapeStyle(.tint) : AnyShapeStyle(.fill.tertiary), in: .capsule)
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(only ? .isSelected : [])
            Spacer(minLength: 0)
            Button("Mark All Read") {
                model.setRead(true, unread)
                withAnimation(.snappy) { only = false }
            }
            .font(.subheadline)
            .disabled(unread.isEmpty)
        }
        .listRowBackground(Color.clear)
        .listRowInsets(EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 4))
        .listRowSeparator(.hidden)
    }
}
