import SheprdRemoteCore
import SwiftUI

/// Home B: every agent in the order the person chose, a question answered
/// in place, with the Workspaces tab's search above it.
struct HomeTab: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @AppStorage(HomeRow.sortKey) private var sort = AgentSort.priority
    @Binding var pairing: Bool
    @State private var query = ""
    /// Lists only agents with a reply no one read.
    @AppStorage("homeUnreadOnly") private var unreadOnly = false

    var body: some View {
        let listed = model.agentListFilter()
        let chats = model.fleet.chatItems.filter { listed($0.computer, $0.workspace) }
        let agents = model.fleet.agentTerminals.filter { listed($0.computer, $0.workspace) }
        let unread = (chats.map(HomeRow.chat) + agents.map(HomeRow.agent))
            .compactMap(\.readTarget).filter(\.unread).map(\.key)
        let sections = HomeRow.sections(
            sort,
            chats: unreadOnly ? chats.filter(\.unread) : chats,
            agents: unreadOnly ? agents.filter(\.unread) : agents
        )
        Group {
            if query.isEmpty { self.chats(sections, unread: unread) } else { SearchResults(query: query, workspacesAsRoutes: true) }
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Workspaces, chats, terminals")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { window?.home = true }
        .onDisappear { window?.home = false }
        #if DEBUG
        .task { if let text = UserDefaults.standard.string(forKey: "homeSearch") { query = text } }
        #endif
        .toolbar {
            ToolbarItem(placement: .topBarLeading) { AgentSortMenu(sort: $sort) }
            ToolbarItem(placement: .topBarLeading) { AgentFilterMenu() }
            if query.isEmpty, sections.count > 1 {
                ToolbarItem(placement: .topBarTrailing) {
                    let shown = sections.map { ChatList.key($0, sort: sort) }
                    let folded = model.collapsed.home.allCollapsed(shown)
                    Button(
                        folded ? "Expand All" : "Collapse All",
                        systemImage: folded ? "rectangle.expand.vertical" : "rectangle.compress.vertical"
                    ) {
                        withAnimation(.snappy) { model.collapsed.home.toggleAll(shown) }
                    }
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("Talk to Sheprd", systemImage: "waveform") { model.talk() }
                    .disabled(model.fleet.machines.isEmpty)
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("New Workspace", systemImage: "plus") { model.creating = .workspace(device: nil) }
                    .disabled(model.fleet.machines.isEmpty)
            }
        }
    }

    private func chats(_ sections: [AgentSort.Section<HomeRow>], unread: [PaneKey]) -> some View {
        ChatList(sections: sections, unread: unread, unreadOnly: $unreadOnly)
            .nearbyBanner()
            .fleetOffer()
            .overlay {
                if model.computers.isEmpty {
                    ContentUnavailableView {
                        Label("No computers yet", systemImage: "desktopcomputer")
                    } description: {
                        Text("In Sheprd, open Remote control > Pair device, then scan its code here.")
                    } actions: {
                        Button("Pair a computer") { pairing = true }.buttonStyle(.glassProminent)
                    }
                } else if model.fleet.chatItems.isEmpty, model.fleet.agentTerminals.isEmpty,
                          // Unpaired computers' cards fill Home instead.
                          !model.fleet.machines.contains(where: { $0.status == .revoked }) {
                    ContentUnavailableView("No chats", systemImage: "bubble.left.and.text.bubble.right",
                                           description: Text("Chats and agents running in Sheprd on your computers appear here."))
                }
            }
    }
}
