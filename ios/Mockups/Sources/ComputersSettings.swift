import SwiftUI

// Settings > Computers: showing online/offline status and useful per-computer
// facts (active route, last seen, workload, load, version, tags) using data
// AppModel / Fleet.Machine / Home already carry.
// A: Clean status rows in a single list, preserving today's compact density.
// B: Grouped into Online and Offline sections (and Attention when needed;
//    state 1 shows problem states: unpaired, outdated, connecting, relay ban).
// C: Fleet health & load rows with a top summary strip and CPU/MEM gauges.
// D: Two-column iOS Settings style (identity + activity left, live route right).

// MARK: - Sample Fleet

private enum ActiveRoute: String {
    case local = "Local network"
    case tailscale = "Tailscale"
    case relay = "Relay"

    var shortLabel: String {
        switch self {
        case .local: "Local"
        case .tailscale: "Tailscale"
        case .relay: "Relay"
        }
    }

    var symbol: String {
        switch self {
        case .local: "wifi"
        case .tailscale: "network"
        case .relay: "antenna.radiowaves.left.and.right"
        }
    }

    var tint: Color {
        switch self {
        case .local: .green
        case .tailscale: .green
        case .relay: .teal
        }
    }
}

private enum MachineState {
    case online(ActiveRoute)
    case connecting
    case offline(lastSeen: String)
    case outdated(version: String)
    case revoked
    case relayBanned(String)

    var isOnline: Bool {
        if case .online = self { return true }
        return false
    }

    var needsAttention: Bool {
        switch self {
        case .outdated, .revoked, .relayBanned: true
        case .online, .connecting, .offline: false
        }
    }
}

private struct MockComputer: Identifiable {
    var id: String { name }
    var name: String
    var symbol: String
    var via: String? = nil
    var configuredReach: String
    var state: MachineState
    var version: String
    var workspaces: Int = 0
    var working: Int = 0
    var needsYou: Int = 0
    var cpu: Int? = nil
    var memory: Int? = nil
    var tags: [String] = []
    var nearDesk: Bool = false
}

private let fleetSample: [MockComputer] = [
    MockComputer(
        name: "m4max", symbol: "laptopcomputer",
        configuredReach: "Tailscale · Relay",
        state: .online(.local), version: "0.15.2",
        workspaces: 3, working: 2, needsYou: 1, cpu: 38, memory: 54,
        tags: ["work"], nearDesk: true
    ),
    MockComputer(
        name: "studio", symbol: "macstudio",
        configuredReach: "Tailscale · Relay",
        state: .online(.tailscale), version: "0.15.2",
        workspaces: 5, working: 4, needsYou: 2, cpu: 62, memory: 71,
        tags: ["hub"]
    ),
    MockComputer(
        name: "9950x3d", symbol: "server.rack", via: "studio",
        configuredReach: "via studio",
        state: .online(.tailscale), version: "0.15.2",
        workspaces: 2, working: 1, needsYou: 0, cpu: 84, memory: 46,
        tags: ["gpu", "linux"]
    ),
    MockComputer(
        name: "framework", symbol: "server.rack", via: "studio",
        configuredReach: "via studio · Relay",
        state: .online(.relay), version: "0.14.9",
        workspaces: 1, working: 0, needsYou: 0, cpu: 12, memory: 34
    ),
    MockComputer(
        name: "m5max", symbol: "laptopcomputer",
        configuredReach: "Tailscale · Relay",
        state: .offline(lastSeen: "2h ago"), version: "0.15.2",
        workspaces: 2
    ),
    MockComputer(
        name: "m2max", symbol: "laptopcomputer",
        configuredReach: "Local network · Relay",
        state: .offline(lastSeen: "Nov 7"), version: "0.14.8",
        workspaces: 1
    ),
    MockComputer(
        name: "penso-m6-1", symbol: "laptopcomputer",
        configuredReach: "Tailscale · Relay",
        state: .offline(lastSeen: "Nov 7"), version: "0.15.1",
        workspaces: 1
    ),
]

private let attentionSample: [MockComputer] = [
    MockComputer(
        name: "m4max", symbol: "laptopcomputer",
        configuredReach: "Tailscale · Relay",
        state: .online(.local), version: "0.15.2",
        workspaces: 3, working: 2, needsYou: 1, cpu: 38, memory: 54,
        tags: ["work"], nearDesk: true
    ),
    MockComputer(
        name: "studio", symbol: "macstudio",
        configuredReach: "Tailscale · Relay",
        state: .connecting, version: "0.15.2",
        workspaces: 5
    ),
    MockComputer(
        name: "framework", symbol: "server.rack", via: "studio",
        configuredReach: "via studio · Relay",
        state: .outdated(version: "0.12.4"), version: "0.12.4"
    ),
    MockComputer(
        name: "m2max", symbol: "laptopcomputer",
        configuredReach: "Local network · Relay",
        state: .revoked, version: "0.14.8"
    ),
    MockComputer(
        name: "penso-m6-1", symbol: "laptopcomputer",
        configuredReach: "Tailscale · Relay",
        state: .relayBanned("relay.sheprd.org"), version: "0.15.1"
    ),
    MockComputer(
        name: "m5max", symbol: "laptopcomputer",
        configuredReach: "Tailscale · Relay",
        state: .offline(lastSeen: "2h ago"), version: "0.15.2"
    ),
]

private let forgetFooter = "Forgetting a computer removes its chats from this iPhone. It does not stop Sheprd there."

// MARK: - Shared Chrome & Small Parts

private enum SettingsMockTab: Hashable { case home, workspaces, prs, settings }

private struct ComputersSettingsFrame<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        TabView(selection: .constant(SettingsMockTab.settings)) {
            Tab("Home", systemImage: "house.fill", value: SettingsMockTab.home) {
                Text("Home")
            }
            .badge(15)
            Tab("Workspaces", systemImage: "square.stack.3d.up", value: SettingsMockTab.workspaces) {
                Text("Workspaces")
            }
            Tab("PRs", systemImage: "arrow.triangle.pull", value: SettingsMockTab.prs) {
                Text("PRs")
            }
            Tab("Settings", systemImage: "gearshape.fill", value: SettingsMockTab.settings) {
                NavigationStack(path: .constant([1])) {
                    List { Text("Settings") }
                        .navigationTitle("Settings")
                        .navigationBarTitleDisplayMode(.inline)
                        .navigationDestination(for: Int.self) { _ in
                            List { content }
                                .navigationTitle("Computers")
                                .navigationBarTitleDisplayMode(.inline)
                                .toolbar {
                                    ToolbarItem(placement: .topBarTrailing) {
                                        Button("Edit") {}
                                    }
                                }
                        }
                }
            }
        }
    }
}

private struct MiniTagPill: View {
    var tag: String

    private var color: Color {
        switch tag {
        case "work": .blue
        case "hub": .purple
        case "gpu": .orange
        case "linux": .teal
        default: .indigo
        }
    }

    var body: some View {
        Text(tag)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 6).padding(.vertical, 1.5)
            .background(color.opacity(0.16), in: .capsule)
    }
}

private struct MiniLoadGauge: View {
    var label: String
    var percent: Int

    var body: some View {
        HStack(spacing: 4) {
            Text(label).font(.caption2.weight(.semibold))
            Capsule().fill(.quaternary).frame(width: 30, height: 4.5)
                .overlay(alignment: .leading) {
                    Capsule().fill(percent >= 80 ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                        .frame(width: 30 * CGFloat(min(max(percent, 0), 100)) / 100, height: 4.5)
                }
            Text("\(percent)%").font(.caption2.monospacedDigit())
        }
        .fixedSize(horizontal: true, vertical: false)
        .foregroundStyle(.secondary)
    }
}

private struct NeedsYouPill: View {
    var count: Int

    var body: some View {
        Text(count == 1 ? "1 needs you" : "\(count) need you")
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(Color.red, in: .capsule)
            .fixedSize()
    }
}

private struct DeviceBadgeIcon: View {
    var symbol: String
    var state: MachineState

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 16, weight: .medium))
            .foregroundStyle(state.isOnline ? .primary : .secondary)
            .frame(width: 32, height: 32)
            .background(
                state.isOnline ? Color.primary.opacity(0.08) : Color.secondary.opacity(0.08),
                in: RoundedRectangle(cornerRadius: 8)
            )
            .overlay(alignment: .bottomTrailing) {
                badge
                    .padding(1.5)
                    .background(Color(.secondarySystemGroupedBackground), in: .circle)
                    .offset(x: 3, y: 3)
            }
    }

    @ViewBuilder
    private var badge: some View {
        switch state {
        case let .online(route):
            Circle().fill(route.tint).frame(width: 8, height: 8)
        case .connecting:
            Circle().fill(.yellow).frame(width: 8, height: 8)
        case .offline:
            Circle().fill(.secondary.opacity(0.5)).frame(width: 8, height: 8)
        case .outdated:
            Image(systemName: "arrow.up.circle.fill").font(.system(size: 10)).foregroundStyle(.orange)
        case .revoked, .relayBanned:
            Image(systemName: "exclamationmark.circle.fill").font(.system(size: 10)).foregroundStyle(.red)
        }
    }
}

// MARK: - Variant A: Clean Status Rows (single list, minimal footprint)

struct ComputersSettingsClean: View {
    var body: some View {
        ComputersSettingsFrame {
            Section {
                ForEach(fleetSample) { computer in
                    NavigationLink {} label: {
                        CleanComputerRow(computer: computer)
                    }
                }
                Button("Pair a Computer", systemImage: "plus") {}
            } header: {
                Text("4 of 7 Online")
            } footer: {
                Text(forgetFooter)
            }
        }
    }
}

private struct CleanComputerRow: View {
    var computer: MockComputer

    var body: some View {
        HStack(spacing: 11) {
            statusGlyph
                .frame(width: 14)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(computer.name)
                        .foregroundStyle(computer.state.isOnline ? .primary : .secondary)
                        .lineLimit(1)
                    if let via = computer.via {
                        Text("via \(via)")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 4)
                    trailing
                }
                subtitle
                    .font(.caption)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 1)
    }

    @ViewBuilder
    private var statusGlyph: some View {
        switch computer.state {
        case let .online(route):
            Circle().fill(route.tint).frame(width: 8, height: 8)
        case .connecting:
            ProgressView().controlSize(.mini)
        case .offline:
            Circle().strokeBorder(.secondary.opacity(0.5), lineWidth: 1.5).frame(width: 8, height: 8)
        case .outdated:
            Image(systemName: "arrow.up.circle.fill").font(.caption).foregroundStyle(.orange)
        case .revoked, .relayBanned:
            Image(systemName: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.red)
        }
    }

    @ViewBuilder
    private var subtitle: some View {
        switch computer.state {
        case let .online(route):
            HStack(spacing: 5) {
                Text(route.rawValue).foregroundStyle(.secondary)
                Text("·").foregroundStyle(.tertiary)
                Text("v\(computer.version)").foregroundStyle(.tertiary).monospacedDigit()
                if computer.nearDesk {
                    Text("·").foregroundStyle(.tertiary)
                    Text("At desk").foregroundStyle(.blue)
                }
            }
        case let .offline(lastSeen):
            Text("Last seen \(lastSeen) · \(computer.configuredReach)")
                .foregroundStyle(.tertiary)
        case .connecting:
            Text("Connecting…").foregroundStyle(.secondary)
        case .outdated:
            Text("Needs a newer Sheprd · v\(computer.version)").foregroundStyle(.orange)
        case .revoked:
            Text("Unpaired this iPhone").foregroundStyle(.red)
        case let .relayBanned(host):
            Text("\(host) banned this IP").foregroundStyle(.red)
        }
    }

    @ViewBuilder
    private var trailing: some View {
        switch computer.state {
        case .online:
            if computer.needsYou > 0 {
                NeedsYouPill(count: computer.needsYou)
            } else if computer.working > 0 {
                HStack(spacing: 4) {
                    Circle().fill(.yellow).frame(width: 6, height: 6)
                    Text("\(computer.working) working")
                        .font(.caption).foregroundStyle(.secondary).monospacedDigit()
                }
                .fixedSize()
            } else {
                Text("Idle")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        case .offline:
            Text("Offline")
                .font(.caption)
                .foregroundStyle(.tertiary)
        default:
            EmptyView()
        }
    }
}

// MARK: - Variant B: Grouped by Status (Online / Offline / Attention)

struct ComputersSettingsGrouped: View {
    var state: Int

    private var list: [MockComputer] { state == 1 ? attentionSample : fleetSample }
    private var attention: [MockComputer] { list.filter { $0.state.needsAttention } }
    private var online: [MockComputer] { list.filter { $0.state.isOnline || isConnecting($0.state) } }
    private var offline: [MockComputer] {
        list.filter {
            if case .offline = $0.state { return true }
            return false
        }
    }

    private func isConnecting(_ state: MachineState) -> Bool {
        if case .connecting = state { return true }
        return false
    }

    var body: some View {
        ComputersSettingsFrame {
            if !attention.isEmpty {
                Section {
                    ForEach(attention) { computer in
                        NavigationLink {} label: { GroupedComputerRow(computer: computer) }
                    }
                } header: {
                    Label("Needs Attention · \(attention.count)", systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                }
            }
            if !online.isEmpty {
                Section {
                    ForEach(online) { computer in
                        NavigationLink {} label: { GroupedComputerRow(computer: computer) }
                    }
                } header: {
                    Text("Online · \(online.count)")
                }
            }
            if !offline.isEmpty {
                Section {
                    ForEach(offline) { computer in
                        NavigationLink {} label: { GroupedComputerRow(computer: computer) }
                    }
                } header: {
                    Text("Offline · \(offline.count)")
                }
            }
            Section {
                Button("Pair a Computer", systemImage: "plus") {}
            } footer: {
                Text(forgetFooter)
            }
        }
    }
}

private struct GroupedComputerRow: View {
    var computer: MockComputer

    var body: some View {
        HStack(spacing: 12) {
            DeviceBadgeIcon(symbol: computer.symbol, state: computer.state)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .center, spacing: 6) {
                    Text(computer.name)
                        .fontWeight(.medium)
                        .foregroundStyle(computer.state.isOnline || computer.state.needsAttention ? .primary : .secondary)
                        .lineLimit(1)
                    if let via = computer.via {
                        Text("via \(via)")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 4)
                    trailing
                }
                detailLine
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private var detailLine: some View {
        switch computer.state {
        case let .online(route):
            HStack(spacing: 4) {
                Image(systemName: route.symbol)
                    .font(.caption2)
                    .foregroundStyle(route == .relay ? .teal : .secondary)
                Text(route.rawValue)
                    .foregroundStyle(route == .relay ? .teal : .secondary)
                Text("·").foregroundStyle(.tertiary)
                Text(computer.workspaces == 1 ? "1 workspace" : "\(computer.workspaces) workspaces")
                    .foregroundStyle(.secondary)
                Text("·").foregroundStyle(.tertiary)
                Text("v\(computer.version)").foregroundStyle(.tertiary).monospacedDigit()
            }
            .font(.caption)
            .lineLimit(1)
        case .connecting:
            HStack(spacing: 5) {
                ProgressView().controlSize(.mini)
                Text("Connecting… · \(computer.configuredReach)")
                    .font(.caption).foregroundStyle(.secondary)
            }
        case .offline:
            Text("\(computer.configuredReach) · v\(computer.version)")
                .font(.caption).foregroundStyle(.tertiary).lineLimit(1)
        case let .outdated(version):
            Text("Needs a newer Sheprd (runs v\(version))")
                .font(.caption).foregroundStyle(.orange).lineLimit(1)
        case .revoked:
            Text("Unpaired this iPhone · tap to pair again")
                .font(.caption).foregroundStyle(.red).lineLimit(1)
        case let .relayBanned(host):
            Text("\(host) banned your IP")
                .font(.caption).foregroundStyle(.red).lineLimit(1)
        }
    }

    @ViewBuilder
    private var trailing: some View {
        switch computer.state {
        case .online:
            if computer.needsYou > 0 {
                NeedsYouPill(count: computer.needsYou)
            } else if computer.working > 0 {
                Text("\(computer.working) active")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.yellow)
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(Color.yellow.opacity(0.14), in: .capsule)
                    .fixedSize()
            }
        case let .offline(lastSeen):
            Text("Seen \(lastSeen)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.tertiary)
                .fixedSize()
        default:
            EmptyView()
        }
    }
}

// MARK: - Variant C: Fleet Health & Load (power-user / ops dashboard)

struct ComputersSettingsHealth: View {
    var body: some View {
        ComputersSettingsFrame {
            Section {
                HStack(spacing: 8) {
                    SummaryChip(dot: .green, value: "4", label: "Online")
                    SummaryChip(dot: .secondary.opacity(0.5), value: "3", label: "Offline")
                    SummaryChip(dot: .yellow, value: "7", label: "Working")
                    SummaryChip(dot: .red, value: "3", label: "Waiting")
                }
                .listRowInsets(EdgeInsets(top: 2, leading: 0, bottom: 2, trailing: 0))
                .listRowBackground(Color.clear)
            }
            Section {
                ForEach(fleetSample) { computer in
                    NavigationLink {} label: { HealthComputerRow(computer: computer) }
                }
                Button("Pair a Computer", systemImage: "plus") {}
            } footer: {
                Text(forgetFooter)
            }
        }
    }
}

private struct SummaryChip: View {
    var dot: Color
    var value: String
    var label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Circle().fill(dot).frame(width: 7, height: 7)
                Text(value).font(.headline.monospacedDigit())
            }
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 10).padding(.vertical, 8)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 12))
    }
}

private struct HealthComputerRow: View {
    var computer: MockComputer

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 7) {
                Image(systemName: computer.symbol)
                    .font(.subheadline)
                    .foregroundStyle(computer.state.isOnline ? .primary : .tertiary)
                    .frame(width: 18)
                Text(computer.name)
                    .fontWeight(.medium)
                    .foregroundStyle(computer.state.isOnline ? .primary : .secondary)
                    .lineLimit(1)
                Text("v\(computer.version)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.tertiary)
                Spacer(minLength: 4)
                routeBadge
            }

            if computer.state.isOnline {
                HStack(spacing: 10) {
                    if let cpu = computer.cpu, let mem = computer.memory {
                        MiniLoadGauge(label: "CPU", percent: cpu)
                        MiniLoadGauge(label: "MEM", percent: mem)
                    }
                    Spacer(minLength: 0)
                    if computer.needsYou > 0 {
                        NeedsYouPill(count: computer.needsYou)
                    } else if computer.working > 0 {
                        Text("\(computer.working) working")
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.yellow)
                            .fixedSize()
                    }
                }
                if !computer.tags.isEmpty || computer.via != nil || computer.nearDesk {
                    HStack(spacing: 5) {
                        if let via = computer.via {
                            Text("via \(via)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        if computer.nearDesk {
                            HStack(spacing: 3) {
                                Image(systemName: "dot.radiowaves.left.and.right")
                                Text("At desk")
                            }
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.blue)
                        }
                        ForEach(computer.tags, id: \.self) { MiniTagPill(tag: $0) }
                    }
                }
            } else if case let .offline(lastSeen) = computer.state {
                Text("Last seen \(lastSeen) · \(computer.configuredReach)")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private var routeBadge: some View {
        switch computer.state {
        case let .online(route):
            HStack(spacing: 4) {
                Circle().fill(route.tint).frame(width: 6, height: 6)
                Text(route.shortLabel)
            }
            .font(.caption2.weight(.semibold))
            .foregroundStyle(route.tint)
            .padding(.horizontal, 7).padding(.vertical, 2.5)
            .background(route.tint.opacity(0.14), in: .capsule)
            .fixedSize()
        case .offline:
            Text("Offline")
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 7).padding(.vertical, 2.5)
                .background(Color.secondary.opacity(0.14), in: .capsule)
                .fixedSize()
        default:
            EmptyView()
        }
    }
}

// MARK: - Variant D: Two-Column Connection-First (iOS Bluetooth/Wi-Fi Settings Style)

struct ComputersSettingsConnection: View {
    var body: some View {
        ComputersSettingsFrame {
            Section {
                ForEach(fleetSample) { computer in
                    NavigationLink {} label: { ConnectionFirstRow(computer: computer) }
                }
                Button("Pair a Computer", systemImage: "plus") {}
            } header: {
                HStack {
                    Text("Paired Computers")
                    Spacer()
                    Text("4 Online").foregroundStyle(.green)
                }
            } footer: {
                Text(forgetFooter)
            }
        }
    }
}

private struct ConnectionFirstRow: View {
    var computer: MockComputer

    private var iconFill: Color {
        switch computer.state {
        case let .online(route): route.tint.opacity(0.85)
        default: Color.secondary.opacity(0.2)
        }
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: computer.symbol)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(computer.state.isOnline ? .white : .secondary)
                .frame(width: 28, height: 28)
                .background(iconFill, in: RoundedRectangle(cornerRadius: 7))

            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    Text(computer.name)
                        .foregroundStyle(computer.state.isOnline ? .primary : .secondary)
                        .lineLimit(1)
                    if let via = computer.via {
                        Text("via \(via)")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                    if computer.needsYou > 0 {
                        Text("\(computer.needsYou)")
                            .font(.caption2.weight(.bold).monospacedDigit())
                            .foregroundStyle(.white)
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(Color.red, in: .capsule)
                    }
                }
                leftSubtitle
                    .font(.caption)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            rightStatus
        }
        .padding(.vertical, 1)
    }

    @ViewBuilder
    private var leftSubtitle: some View {
        if computer.state.isOnline {
            let activity: String = {
                if computer.working > 0 {
                    return "\(computer.working) working · \(computer.workspaces) ws"
                }
                return computer.workspaces == 1 ? "1 workspace" : "\(computer.workspaces) workspaces"
            }()
            Text("\(activity) · v\(computer.version)")
                .foregroundStyle(.secondary)
        } else {
            Text("\(computer.configuredReach) · v\(computer.version)")
                .foregroundStyle(.tertiary)
        }
    }

    @ViewBuilder
    private var rightStatus: some View {
        switch computer.state {
        case let .online(route):
            VStack(alignment: .trailing, spacing: 1) {
                HStack(spacing: 4) {
                    Circle().fill(route.tint).frame(width: 6, height: 6)
                    Text(route.shortLabel)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(route.tint)
                }
                if let cpu = computer.cpu {
                    Text("CPU \(cpu)%")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(cpu >= 80 ? AnyShapeStyle(.orange) : AnyShapeStyle(.tertiary))
                }
            }
            .fixedSize()
        case let .offline(lastSeen):
            VStack(alignment: .trailing, spacing: 1) {
                Text("Offline")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(lastSeen)
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.tertiary)
            }
            .fixedSize()
        default:
            EmptyView()
        }
    }
}
