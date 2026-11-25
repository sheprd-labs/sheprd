import Foundation
import SheprdRemoteCore

/// A made-up fleet for someone with no computer paired yet, such as App
/// Review: two computers, their workspaces, chats with a question waiting,
/// and a terminal (`DemoData`). Nothing in it reaches a network. `Fleet`
/// shows these homes as if the computers had sent them, starts no link for
/// them, and answers every call to them with `FleetError.demo`; pairing a
/// real computer ends it.
enum DemoFleet {
    static let computers = DemoData.computers

    static func owns(_ device: String) -> Bool { DemoData.owns(device) }

    /// The machines `Fleet` shows while the demo runs.
    static func machines(now: Date = .now) -> [Fleet.Machine] {
        computers.map { computer in
            Fleet.Machine(computer: computer, status: .online(origin: DemoData.origin),
                          home: DemoData.home(of: computer.device, now: now), stale: false, updated: now, watching: true)
        }
    }

    /// Opening a chat reads its reply in the in-memory demo, just as a
    /// paired computer reports it read in the next home it sends.
    static func markOpened(_ key: PaneKey, in machines: inout [Fleet.Machine]) {
        guard let machine = machines.firstIndex(where: { $0.id == key.device }),
              var home = machines[machine].home else { return }
        for workspace in home.workspaces.indices {
            if let chat = home.workspaces[workspace].chats.firstIndex(where: { $0.pane == key.pane && $0.unread == true }) {
                home.workspaces[workspace].chats[chat].unread = false
                machines[machine].home = home
                return
            }
        }
    }

    /// What a demo chat shows when opened; nil for any other chat. The
    /// longer ones are the simulator loop's samples.
    static func transcript(_ key: PaneKey, now: Date = .now) -> Transcript? {
        guard owns(key.device) else { return nil }
        return switch key.pane {
        case "c1": .sample
        case "c4": .activitySample
        default: DemoData.transcript(pane: key.pane, now: now)
        }
    }

    /// What a demo terminal shows; nil for any other terminal.
    static func screen(_ key: PaneKey) -> TermScreen? {
        owns(key.device) ? DemoData.screen(pane: key.pane) : nil
    }
}
