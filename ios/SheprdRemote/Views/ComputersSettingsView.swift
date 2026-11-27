import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings > Computers: fleet health summary strip above the paired
/// computers, each showing live connection route or offline/problem status,
/// CPU/MEM load, active agents, version, desk presence, and tags.
struct ComputersSettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var forgetting: Computer?
    @State private var editMode = EditMode.inactive

    private var summary: FleetHealth.Summary {
        model.fleetHealthSummary
    }

    var body: some View {
        List {
            if !model.computers.isEmpty {
                Section {
                    summaryChips
                        .listRowInsets(EdgeInsets(top: 2, leading: 0, bottom: 2, trailing: 0))
                        .listRowBackground(Color.clear)
                }
            }
            Section {
                ForEach(model.computers) { computer in
                    NavigationLink {
                        ComputerScreen(device: computer.device).makesRoomForConnectionToast()
                    } label: {
                        ComputerHealthRow(computer: computer)
                    }
                    .swipeActions {
                        // Not `.destructive`: that removes the row before
                        // the confirmation answers.
                        Button("Forget", systemImage: "trash") { forgetting = computer }.tint(.red)
                    }
                }
                .onDelete { offsets in forgetting = offsets.first.map { model.computers[$0] } }
                Button("Pair a Computer", systemImage: "plus") { window?.pairing = true }
            } footer: {
                if !model.computers.isEmpty {
                    Text("Forgetting a computer removes its chats from this \(UIDevice.current.model). It does not stop Sheprd there.")
                }
            }
        }
        .refreshable { model.fleet.networkChanged() }
        .agentGuideScrolling()
        .navigationTitle("Computers")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { if !model.computers.isEmpty { EditButton() } }
        .forgetConfirmation($forgetting)
        .environment(\.editMode, $editMode)
        #if DEBUG
        .task { if UserDefaults.standard.bool(forKey: "editComputers") { editMode = .active } }
        #endif
    }

    @ViewBuilder
    private var summaryChips: some View {
        if dynamicTypeSize.isAccessibilitySize {
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                chipViews
            }
        } else {
            HStack(spacing: 8) {
                chipViews
            }
        }
    }

    @ViewBuilder
    private var chipViews: some View {
        FleetSummaryChip(dot: .green, value: summary.online, label: "Online")
        FleetSummaryChip(dot: .secondary.opacity(0.5), value: summary.offline, label: "Offline")
        FleetSummaryChip(dot: Home.AgentStatus.working.color, value: summary.working, label: "Working")
        FleetSummaryChip(dot: Home.AgentStatus.blocked.color, value: summary.waiting, label: "Waiting")
    }
}

extension AppModel {
    /// Live fleet health counts across paired computers for Settings. Uses
    /// actual `status` (not cached `presentedStatus`) and marks stale homes
    /// so cached counts never look live while reconnecting.
    var fleetHealthSummary: FleetHealth.Summary {
        let machinesById = Dictionary(uniqueKeysWithValues: fleet.machines.map { ($0.id, $0) })
        let entries = computers.map { computer in
            let machine = machinesById[computer.device]
            return FleetHealth.Entry(
                status: machine?.status ?? .connecting,
                stale: machine?.stale ?? true,
                needsUpdate: machine?.needsUpdate == true,
                working: machine?.home?.workingAgents ?? 0,
                waiting: machine?.home?.waiting ?? 0
            )
        }
        return FleetHealth.Summary(entries: entries)
    }
}

/// One of the four fleet health chips at the top of Settings > Computers.
private struct FleetSummaryChip: View {
    var dot: Color
    var value: Int
    var label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Circle().fill(dot).frame(width: 7, height: 7)
                Text("\(value)").font(.headline.monospacedDigit())
            }
            Text(label).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 10).padding(.vertical, 8)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(value) \(label.lowercased())")
    }
}

/// One paired computer in Settings: live route/status badge, version, load
/// gauges and active agent counts when online, or last-seen and configured
/// routes when offline.
private struct ComputerHealthRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    var computer: Computer

    private var machine: Fleet.Machine? {
        model.fleet.machines.first { $0.id == computer.device }
    }

    private var status: LinkStatus {
        machine?.status ?? .connecting
    }

    private var isOnline: Bool {
        if case .online = status { return true }
        return false
    }

    private var liveHome: Home? {
        guard isOnline, let machine, !machine.stale else { return nil }
        return machine.home
    }

    private var needsUpdate: Bool {
        status == .outdated || machine?.needsUpdate == true
    }

    private var reachText: String? {
        if let vouching = model.vouching(computer) {
            let via = "via \(model.displayName(vouching))"
            return computer.relay == nil ? via : "\(via) · \(Computer.relayLabel)"
        }
        return computer.reachLabel
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 4) {
                    headerIdentity
                    statusBadge
                }
            } else {
                HStack(spacing: 7) {
                    headerIdentity
                    Spacer(minLength: 4)
                    statusBadge
                }
            }

            if isOnline {
                onlineDetails
            } else {
                nonOnlineDetails
            }
        }
        .padding(.vertical, 2)
    }

    private var headerIdentity: some View {
        HStack(spacing: 7) {
            Image(systemName: computer.via == nil ? "laptopcomputer" : "server.rack")
                .font(.subheadline)
                .foregroundStyle(iconStyle)
                .frame(width: 18)
            Text(model.displayName(computer))
                .fontWeight(.medium)
                .foregroundStyle(isOnline || hasProblem ? .primary : .secondary)
                .lineLimit(1)
            if let version = FleetHealth.versionLabel(machine?.home?.version) {
                Text(version)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
    }

    private var hasProblem: Bool {
        switch status {
        case .revoked, .outdated, .relayBanned: true
        case .online, .connecting, .offline: needsUpdate
        }
    }

    private var iconStyle: AnyShapeStyle {
        switch status {
        case .online:
            needsUpdate ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary)
        case .outdated:
            AnyShapeStyle(.orange)
        case .revoked, .relayBanned:
            AnyShapeStyle(.red)
        case .connecting, .offline:
            AnyShapeStyle(.tertiary)
        }
    }

    @ViewBuilder
    private var statusBadge: some View {
        switch status {
        case let .online(origin):
            if needsUpdate {
                badgePill(color: .orange) {
                    Image(systemName: "arrow.up.circle.fill")
                    Text("Update")
                }
                .accessibilityLabel("Needs a newer Sheprd")
            } else {
                let kind = FleetHealth.routeKind(origin: origin, throughRelay: model.fleet.throughRelay(computer.device))
                let tint: Color = kind == .relay ? .teal : .green
                badgePill(color: tint) {
                    Circle().fill(tint).frame(width: 6, height: 6)
                    Text(kind.label)
                }
                .accessibilityLabel("Connected via \(kind.label)")
            }
        case .connecting:
            badgePill(color: .secondary) {
                ProgressView().controlSize(.mini)
                Text("Connecting")
            }
        case .offline:
            badgePill(color: .secondary) {
                Text("Offline")
            }
        case .outdated:
            badgePill(color: .orange) {
                Image(systemName: "arrow.up.circle.fill")
                Text("Update")
            }
            .accessibilityLabel("Needs a newer Sheprd")
        case .revoked:
            badgePill(color: .red) {
                Image(systemName: "exclamationmark.triangle.fill")
                Text("Unpaired")
            }
        case .relayBanned:
            badgePill(color: .red) {
                Image(systemName: "nosign")
                Text("Relay ban")
            }
        }
    }

    private func badgePill<Content: View>(color: Color, @ViewBuilder content: () -> Content) -> some View {
        HStack(spacing: 4) {
            content()
        }
        .font(.caption2.weight(.semibold))
        .foregroundStyle(color)
        .padding(.horizontal, 7).padding(.vertical, 2.5)
        .background(color.opacity(0.14), in: .capsule)
        .fixedSize()
    }

    @ViewBuilder
    private var onlineDetails: some View {
        let load = liveHome?.load
        let waiting = liveHome?.waiting ?? 0
        let working = liveHome?.workingAgents ?? 0

        if needsUpdate {
            ComputerStatusLine(status: .outdated).font(.caption)
        }

        if load != nil || waiting > 0 || working > 0 {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) {
                    if let load { loadGauges(load) }
                    Spacer(minLength: 0)
                    activityBadge(waiting: waiting, working: working)
                }
                VStack(alignment: .leading, spacing: 4) {
                    if let load {
                        ViewThatFits(in: .horizontal) {
                            loadGauges(load)
                            VStack(alignment: .leading, spacing: 2) {
                                LoadGauge(label: "CPU", percent: load.cpu)
                                LoadGauge(label: "MEM", percent: load.memory)
                            }
                        }
                    }
                    activityBadge(waiting: waiting, working: working)
                }
            }
        }

        let vouching = model.vouching(computer)
        let near = model.presence.isNear(computer.fingerprint)
        if vouching != nil || near || !computer.tags.isEmpty {
            PillFlow(spacing: 5) {
                if let vouching {
                    Text("via \(model.displayName(vouching))")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .padding(.vertical, 3)
                }
                if near {
                    HStack(spacing: 3) {
                        Image(systemName: "dot.radiowaves.left.and.right")
                        Text("At desk")
                    }
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.blue)
                    .padding(.vertical, 3)
                }
                ForEach(computer.tags, id: \.self) { tag in
                    TagPill(tag: tag)
                }
            }
        }
    }

    private func loadGauges(_ load: Home.Load) -> some View {
        HStack(spacing: 10) {
            LoadGauge(label: "CPU", percent: load.cpu)
            LoadGauge(label: "MEM", percent: load.memory)
        }
    }

    @ViewBuilder
    private func activityBadge(waiting: Int, working: Int) -> some View {
        if waiting > 0 {
            Text(waiting == 1 ? "1 needs you" : "\(waiting) need you")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 7).padding(.vertical, 2)
                .background(Home.AgentStatus.blocked.color, in: .capsule)
                .fixedSize()
        } else if working > 0 {
            Text("\(working) working")
                .font(.caption2.weight(.medium))
                .foregroundStyle(Home.AgentStatus.working.color)
                .fixedSize()
        }
    }

    @ViewBuilder
    private var nonOnlineDetails: some View {
        switch status {
        case .revoked:
            ComputerStatusLine(status: .revoked).font(.caption)
        case .outdated:
            ComputerStatusLine(status: .outdated).font(.caption)
        case let .relayBanned(relay):
            ComputerStatusLine(status: .relayBanned(relay)).font(.caption)
        case .connecting:
            if let reachText {
                Text(reachText).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
        case .offline:
            TimelineView(.everyMinute) { context in
                if let detail = FleetHealth.offlineDetail(lastSeen: machine?.updated, reach: reachText, now: context.date) {
                    Text(detail).font(.caption).foregroundStyle(.tertiary).lineLimit(1)
                }
            }
        case .online:
            EmptyView()
        }
    }
}
