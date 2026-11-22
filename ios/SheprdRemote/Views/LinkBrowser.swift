import SheprdRemoteCore
import SwiftUI
import WebKit

extension View {
    /// Opens tapped links whose scheme is in `schemes` in the in-app
    /// browser, in a sheet over this view; others are dropped.
    func browsesLinks(schemes: Set<String> = BrowserNavigation.webSchemes) -> some View {
        modifier(LinkBrowsing(schemes: schemes))
    }
}

private struct LinkBrowsing: ViewModifier {
    var schemes: Set<String>
    @State private var link: BrowsedLink?

    func body(content: Content) -> some View {
        content
            .environment(\.openURL, OpenURLAction { url in
                guard schemes.contains(url.scheme?.lowercased() ?? "") else { return .discarded }
                link = BrowsedLink(url: url)
                return .handled
            })
            .sheet(item: $link) { link in
                LinkBrowser(url: link.url).presentationSizing(.page)
            }
    }
}

/// A sheet's link: a new one each tap, so tapping the same link again
/// after closing it opens a fresh page.
private struct BrowsedLink: Identifiable {
    let id = UUID()
    let url: URL
}

/// A web page in a sheet. Links it follows stack up in its history, which
/// the back button (hold it for the list) and an edge swipe walk; Safari
/// takes the page currently shown.
struct LinkBrowser: View {
    let url: URL
    @Environment(\.dismiss) private var dismiss
    @State private var page: WebPage
    /// Why the last load the sheet started failed, until another starts.
    @State private var failure: Error?

    init(url: URL) {
        self.url = url
        let opener = Opener()
        let page = WebPage(navigationDecider: Decider(opener: opener))
        opener.page = page
        _page = State(initialValue: page)
    }

    var body: some View {
        NavigationStack {
            WebView(page)
                .webViewBackForwardNavigationGestures(.enabled)
                .overlay(alignment: .top) {
                    ProgressView(value: page.estimatedProgress)
                        .progressViewStyle(.linear)
                        .opacity(page.isLoading ? 1 : 0)
                        .animation(.easeOut, value: page.isLoading)
                }
                .overlay {
                    if let failure {
                        ContentUnavailableView {
                            Label("Can't Open This Page", systemImage: "wifi.exclamationmark")
                        } description: {
                            Text(failure.localizedDescription)
                        } actions: {
                            Button("Try Again", action: retry).buttonStyle(.borderedProminent)
                        }
                        .background(.background)
                    }
                }
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { toolbar }
        }
        .task {
            guard page.url == nil else { return }
            follow(page.load(url))
        }
    }

    /// Watches a load the sheet started, so a page that cannot be reached
    /// says why instead of staying blank.
    private func follow(_ events: some AsyncSequence<WebPage.NavigationEvent, any Error>) {
        failure = nil
        Task {
            do {
                for try await _ in events {}
            } catch {
                let cause = if case let WebPage.NavigationError.failedProvisionalNavigation(inner) = error { inner } else { error }
                if BrowserNavigation.reports(cause as NSError) { failure = cause }
            }
        }
    }

    /// Loads the link again when nothing ever showed, else the page.
    private func retry() {
        if page.url == nil { follow(page.load(url)) } else { follow(page.reload()) }
    }

    /// The page shown now, or the link while it is still loading.
    private var current: URL { page.url ?? url }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .cancellationAction) {
            Button("Done", systemImage: "xmark") { dismiss() }
        }
        ToolbarItem(placement: .principal) {
            // The host leads: the page writes its own title, so a title
            // could name another site.
            VStack(spacing: 1) {
                Label(current.host() ?? current.absoluteString,
                      systemImage: current.scheme?.lowercased() == "https" ? "lock.fill" : "exclamationmark.triangle.fill")
                    .labelStyle(.titleAndIcon).font(.subheadline.weight(.semibold)).lineLimit(1).truncationMode(.head)
                if !page.title.isEmpty {
                    Text(page.title).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
        }
        ToolbarItem(placement: .primaryAction) {
            Button("Open in Safari", systemImage: "safari") { UIApplication.shared.open(current) }
        }
        ToolbarItemGroup(placement: .bottomBar) {
            history(page.backForwardList.backList.reversed(), label: "Back", symbol: "chevron.backward")
            history(page.backForwardList.forwardList, label: "Forward", symbol: "chevron.forward")
            Spacer()
            if page.isLoading {
                Button("Stop", systemImage: "xmark") { page.stopLoading() }
            } else {
                Button("Reload", systemImage: "arrow.clockwise") { follow(page.reload()) }
            }
            ShareLink(item: current)
        }
    }

    /// A step through the history: a tap goes one page, holding lists
    /// every page that way, nearest first.
    private func history(_ items: [WebPage.BackForwardList.Item], label: String, symbol: String) -> some View {
        Menu {
            ForEach(items) { item in
                Button(item.title.flatMap { $0.isEmpty ? nil : $0 } ?? item.url.absoluteString) { follow(page.load(item)) }
            }
        } label: {
            Label(label, systemImage: symbol)
        } primaryAction: {
            if let item = items.first { follow(page.load(item)) }
        }
        .disabled(items.isEmpty)
    }
}

/// How the decider reaches the page it decides for, which is made after it.
@MainActor
private final class Opener {
    weak var page: WebPage?
}

private struct Decider: WebPage.NavigationDeciding {
    let opener: Opener

    func decidePolicy(for action: WebPage.NavigationAction, preferences: inout WebPage.NavigationPreferences) async -> WKNavigationActionPolicy {
        let url = action.request.url
        switch BrowserNavigation.decide(url, mainFrame: action.target?.isMainFrame ?? true, newWindow: action.target == nil) {
        case .allow:
            return .allow
        case .loadHere:
            _ = opener.page?.load(action.request)
            return .cancel
        case .refuse:
            return .cancel
        }
    }
}
