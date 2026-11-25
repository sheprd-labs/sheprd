import SafariServices
import SwiftUI

/// With no computer paired, the first list says so and offers pairing one
/// or exploring the demo (`DemoFleet`); while the demo runs, a bar along
/// its bottom says the computers are made up and leads to pairing.
private struct DemoOffer: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    @State private var installing = false

    /// How to install Sheprd on each platform (website/docs/install.md).
    static let installGuide = URL(string: "https://sheprd.org/docs/install/") ?? URL(filePath: "/")

    func body(content: Content) -> some View {
        content
            .overlay {
                if model.shownComputers.isEmpty {
                    ContentUnavailableView {
                        Label("A companion to Sheprd", systemImage: "desktopcomputer")
                    } description: {
                        // The app shows nothing on its own: say what else it needs.
                        Text("This app requires Sheprd running on your computer or server. Install Sheprd, open Remote control > Pair device, then scan its code here.")
                            // A readable measure across an iPad's window.
                            .frame(maxWidth: 420)
                    } actions: {
                        VStack(spacing: 12) {
                            Button { installing = true } label: {
                                Label("Install Sheprd", systemImage: "arrow.down.circle")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.glass)
                            Button { pairing = true } label: {
                                Label("Pair a Computer", systemImage: "qrcode.viewfinder")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.glassProminent)
                            Button("No computer yet? Explore the Demo") {
                                withAnimation(.snappy) { model.startDemo() }
                            }
                            .buttonStyle(.plain)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .padding(.top, 4)
                        }
                        .controlSize(.large)
                        .frame(maxWidth: 300)
                    }
                }
            }
            .sheet(isPresented: $installing) {
                SafariView(url: Self.installGuide).ignoresSafeArea()
            }
            .safeAreaInset(edge: .bottom) {
                if model.fleet.demo, !Self.barHidden {
                    DemoBar(pairing: $pairing)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.snappy, value: model.fleet.demo)
    }

    /// The store screenshots' `-hideDemoBar YES` (`tools/store_screenshots.sh`).
    private static var barHidden: Bool {
        #if DEBUG
        UserDefaults.standard.bool(forKey: "hideDemoBar")
        #else
        false
        #endif
    }
}

private struct DemoBar: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Demo").font(.subheadline.weight(.semibold))
                Text("Made-up computers. Nothing is sent.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Button("End") { model.endDemo() }
                .buttonStyle(.glass)
            Button("Pair") { pairing = true }
                .buttonStyle(.glassProminent)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .glassEffect(in: .rect(cornerRadius: 20))
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
        .accessibilityElement(children: .contain)
    }
}

/// Safari in a sheet, for pages of ours the app links to.
private struct SafariView: UIViewControllerRepresentable {
    let url: URL

    func makeUIViewController(context: Context) -> SFSafariViewController {
        SFSafariViewController(url: url)
    }

    func updateUIViewController(_ controller: SFSafariViewController, context: Context) {}
}

extension View {
    /// The search of a first list, while there is anything to search.
    @ViewBuilder func searchable(_ query: Binding<String>, when shown: Bool) -> some View {
        if shown {
            searchable(text: query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Workspaces, chats, terminals")
        } else {
            self
        }
    }

    /// The empty list's pairing and demo offer, and the demo's bar.
    func demoOffer(pairing: Binding<Bool>) -> some View {
        modifier(DemoOffer(pairing: pairing))
    }
}
