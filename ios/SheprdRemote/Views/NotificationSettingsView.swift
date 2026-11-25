import ActivityKit
import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings > Notifications: the push relay, alerts held at the desk, Live
/// Activities, and one test push from a source picked in a menu.
struct NotificationSettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(LiveActivityStyleSetting.key, store: LiveActivityStyleSetting.store)
    private var style = LiveActivityStyle.standardChoice.rawValue
    @AppStorage(ActivityGrouping.defaultKey) private var defaultMultiple = false
    @AppStorage(ActivityGrouping.allHostsKey) private var allHosts = false
    @State private var showingStyle = false

    private enum Grouping { case host, agent, shared }

    var body: some View {
        List {
            PushRelaySection()
            if !model.computers.isEmpty {
                Section {
                    NavigationLink(value: AgentSettingsPage.deskHold) {
                        let held = model.computers.count(where: \.presence)
                        LabeledContent("Hold Alerts at the Desk", value: held == 0 ? "Off" : "\(held) of \(model.computers.count)")
                    }
                } footer: {
                    Text("A computer holds phone alerts while you are near it and using it.")
                }
            }
            Section {
                if !ActivityAuthorizationInfo().areActivitiesEnabled {
                    Label("Live Activities are off for Sheprd. Turn them on in Settings.", systemImage: "exclamationmark.triangle")
                }
                Picker("Default grouping", selection: Binding(
                    get: { allHosts ? Grouping.shared : (defaultMultiple ? .agent : .host) },
                    set: { choice in
                        let changed = allHosts != (choice == .shared)
                        allHosts = choice == .shared
                        defaultMultiple = choice == .agent
                        Task { await model.tokens.groupingChanged(sharedChanged: changed) }
                    }
                )) {
                    Text("One per host").tag(Grouping.host)
                    Text("One per agent").tag(Grouping.agent)
                    Text("One for all hosts").tag(Grouping.shared)
                }
                ForEach(model.shownComputers) { computer in
                    ActivityGroupingPicker(computer: computer, label: model.displayName(computer))
                }
                NavigationLink {
                    LiveActivityStyleView().makesRoomForConnectionToast()
                } label: {
                    LabeledContent("Style", value: LiveActivityStyle.stored(style).title)
                }
                .agentHighlight(.liveActivities)
                TestActivityRow()
                #if DEBUG
                Button("Start a sample activity") { model.startSample() }
                if let status = model.sampleStatus {
                    Text(status).font(.footnote).foregroundStyle(.secondary)
                }
                #endif
            } header: {
                Text("Live Activities")
            } footer: {
                Text("One for all hosts shares a card across computers using the default. One per host combines work across all worktrees on that computer. One per agent gives each independent chat or terminal agent its own activity. Delegated workers count toward their coordinator’s progress. Changes apply to new activities. The Private style hides chat names and steps. A computer can also limit detail in its Remote control settings.")
            }
            Section {
                TestPushRow().agentHighlight(.testPush)
            } footer: {
                Text("Sends this phone a sealed notification now and shows whether it arrived.")
            }
        }
        .agentGuideScrolling()
        .navigationTitle("Notifications")
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(isPresented: $showingStyle) { LiveActivityStyleView().makesRoomForConnectionToast() }
        // The simulator loop's `-openLiveActivityStyle YES`.
        .task { if UserDefaults.standard.bool(forKey: "openLiveActivityStyle") { showingStyle = true } }
    }
}

/// Both settings pages edit the same preference for this computer's identity.
struct ActivityGroupingPicker: View {
    @Environment(AppModel.self) private var model
    @AppStorage private var multiple: Bool?
    let computer: Computer
    let label: String

    init(computer: Computer, label: String) {
        self.computer = computer
        self.label = label
        _multiple = AppStorage(ActivityGrouping.key(for: computer.fingerprint))
    }

    var body: some View {
        Picker(label, selection: $multiple) {
            Text("Use default").tag(nil as Bool?)
            Text("One per host").tag(false as Bool?)
            Text("One per agent").tag(true as Bool?)
        }
        .onChange(of: multiple) { old, new in
            let sharedChanged = (old == nil || new == nil) && UserDefaults.standard.bool(forKey: ActivityGrouping.allHostsKey)
            Task { await model.tokens.groupingChanged(sharedChanged: sharedChanged, affected: computer.fingerprint) }
        }
    }
}

/// Settings > Notifications > Delivery: how each paired computer delivers
/// this phone's pushes, as far as its last answer said.
struct PushDeliveryView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(PushRelaySetting.key, store: PushRelaySetting.store) private var pushRelay = true

    var body: some View {
        List {
            Section {
                ForEach(model.computers) { computer in
                    LabeledContent(model.displayName(computer), value: PushRelaySection.label(
                        model.tokens.delivery[computer.fingerprint],
                        reached: !model.tokens.unreached.contains(computer.fingerprint),
                        pushRelay: pushRelay
                    ))
                }
            } footer: {
                Text("A computer with no push set up can turn it on in Sheprd's Remote control settings there.")
            }
        }
        .navigationTitle("Delivery")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Notifications > Hold Alerts at the Desk: the presence switch
/// of every computer, which each computer's page also has.
struct DeskHoldView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            Section {
                ForEach(model.computers) { computer in
                    Toggle(model.displayName(computer), isOn: Binding(
                        get: { computer.presence },
                        set: { model.setPresence($0, for: computer) }
                    ))
                }
            } footer: {
                Text("Over Bluetooth, this phone tells the computer when you are near it, and the computer holds phone alerts while you are there and using it. It also keeps the computer's addresses current. The computer needs presence on in Sheprd's Remote control settings.")
            }
        }
        .navigationTitle("Hold Alerts at the Desk")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Start Test Activity: this phone starts a Live Activity on itself through
/// the push relay (`ActivityTest`), which needs no computer online.
struct TestActivityRow: View {
    @Environment(AppModel.self) private var model
    @AppStorage(PushRelaySetting.key, store: PushRelaySetting.store) private var pushRelay = true

    var body: some View {
        let test = model.activityTest
        VStack(alignment: .leading, spacing: 4) {
            Button {
                Task { await test.start() }
            } label: {
                Label("Start Test Activity", systemImage: "platter.filled.top.iphone")
            }
            .buttonStyle(.borderless)
            .disabled(!pushRelay || test.busy)
            if let status = test.status {
                Text(ActivityTest.text(status)).font(.footnote).foregroundStyle(.secondary)
            } else if !pushRelay {
                Text("Turn on Use the Push Relay to test Live Activities.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
    }
}

/// One Send Test Push button and a menu of where it comes from: this phone
/// through the push relay (`PushTests.sendFromThisPhone`), which needs no
/// computer online, or a paired computer (`PushTests.send(to:)`). Without
/// the push relay the phone cannot push itself, so it is not offered.
struct TestPushRow: View {
    @Environment(AppModel.self) private var model
    @AppStorage(PushRelaySetting.key, store: PushRelaySetting.store) private var pushRelay = true
    @State private var picked: Source?

    enum Source: Hashable {
        case phone
        /// A paired computer, by device id.
        case computer(String)
    }

    var body: some View {
        let sources = (pushRelay ? [Source.phone] : []) + model.computers.map { .computer($0.device) }
        let source = picked.flatMap { sources.contains($0) ? $0 : nil } ?? sources.first
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Button {
                    guard let source else { return }
                    Task { await send(from: source) }
                } label: {
                    Label("Send Test Push", systemImage: "paperplane")
                }
                .buttonStyle(.borderless)
                .disabled(source == nil)
                Spacer()
                if sources.count > 1 {
                    Picker("From", selection: Binding(get: { source }, set: { picked = $0 })) {
                        ForEach(sources, id: \.self) { Text(name(of: $0)).tag(Optional($0)) }
                    }
                    .pickerStyle(.menu)
                    .labelsHidden()
                }
            }
            if let source, let status = status(of: source) {
                Text(PushTests.text(status)).font(.footnote).foregroundStyle(.secondary)
            } else if sources.isEmpty {
                Text("Turn on Use the Push Relay, or pair a computer, to test notifications.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
    }

    private func computer(_ device: String) -> Computer? {
        model.computers.first { $0.device == device }
    }

    private func name(of source: Source) -> String {
        switch source {
        case .phone: "This \(UIDevice.current.model)"
        case let .computer(device): computer(device).map(model.displayName) ?? device
        }
    }

    private func status(of source: Source) -> PushTests.Status? {
        switch source {
        case .phone: model.pushTests.ownStatus
        case let .computer(device): computer(device).flatMap { model.pushTests.status[$0.fingerprint] }
        }
    }

    private func send(from source: Source) async {
        switch source {
        case .phone: await model.pushTests.sendFromThisPhone()
        case let .computer(device):
            if let computer = computer(device) { await model.pushTests.send(to: computer) }
        }
    }
}
