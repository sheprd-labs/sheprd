#if DEBUG
import Darwin
import Foundation
import QuartzCore
import SheprdGitHub
import SheprdRemoteCore
import UIKit

/// Measures the Pull Requests tab, SwiftUI against Luau, on the same data:
/// `-gitHubDemo bench` lists 50 pull requests in each section, the most a
/// GitHub read keeps, and `-prsBench YES` then opens the tab, waits for its
/// first frame with rows, and scrolls it end to end at a steady speed,
/// timing every frame. Pick the engine with `-pullRequestsEngine swift|luau`.
/// The results go to the log and to `Documents/prs-bench.json`
/// (`ios/tools/prs_bench.sh` runs both and averages).
@MainActor
enum PullRequestsBench {
    /// The bench's lists, when asked for.
    nonisolated static var lists: PullRequestLists? {
        guard UserDefaults.standard.string(forKey: "gitHubDemo") == "bench" else { return nil }
        return PullRequestLists(mine: (0..<50).map { pull($0, mine: true) }, reviewRequested: (0..<50).map { pull($0, mine: false) })
    }

    private nonisolated static func pull(_ index: Int, mine: Bool) -> GitHubSearchPull {
        let repos = ["sheprd", "sheprd-apns", "herdr-site", "infra"]
        let titles = [
            "Read pull requests on the phone with a lent GitHub sign-in", "Keep the relay's dial keys after a restart",
            "Docs: GitHub on the phone", "Group tabs by worktree in the iPad sidebar", "Bump relay droplets to 2 vCPU",
            "Teleport: carry checkpoints across hosts and keep their labels when the worktree moves",
        ]
        let reviews: [GitHubPullRequest.ReviewDecision] = [.none, .approved, .changesRequested, .reviewRequired]
        let authors = ["octocat", "hubot", "monalisa", "dependabot"]
        return GitHubSearchPull(
            host: GitHubHost("github.com")!, owner: "sheprd-labs", repo: repos[index % repos.count],
            number: UInt64((mine ? 1000 : 2000) + index), title: titles[index % titles.count], isDraft: index % 9 == 4,
            reviewDecision: reviews[index % reviews.count], author: authors[index % authors.count],
            updatedAt: .now.addingTimeInterval(-Double(index) * 1800), headRefName: index % 5 == 0 ? "feat/ios-native-github" : "bench/\(index)",
            headOwner: "sheprd-labs", checks: .init(failing: index % 4 == 2 ? 1 : 0, pending: index % 3, passed: 3 + index % 11)
        )
    }

    /// Runs the bench when `-prsBench YES` asks for it.
    static func runIfAsked(model: AppModel) async {
        guard UserDefaults.standard.bool(forKey: "prsBench") else { return }
        let engine = UserDefaults.standard.string(forKey: ScreenEngine.pullRequestsKey) ?? "swift"
        // Lets launch settle on Home.
        try? await Task.sleep(for: .seconds(3))
        let before = footprint()
        let opened = CACurrentMediaTime()
        model.pendingTab = .pullRequests
        let frames = FrameClock()
        let firstFrame = await frames.first { list()?.visibleCells.count ?? 0 >= 3 }
        try? await Task.sleep(for: .seconds(1))
        let shown = footprint()
        guard let list = list() else { return report(["engine": engine, "error": "no list"]) }
        let scroll = await frames.scroll(list, passes: 4, pointsPerSecond: 3000)
        let after = footprint()
        report([
            "engine": engine,
            "firstFrameMs": (firstFrame - opened) * 1000,
            "footprintBeforeMB": before, "footprintShownMB": shown, "footprintAfterScrollMB": after,
            "maxFramesPerSecond": Double(UIScreen.main.maximumFramesPerSecond),
            "frames": Double(scroll.frames), "hitches": Double(scroll.hitches),
            "hitchMsPerS": scroll.hitchRatio, "worstFrameMs": scroll.worst * 1000,
            "medianFrameMs": scroll.median * 1000, "seconds": scroll.seconds,
        ])
    }

    /// The tab's list: the largest collection view on screen.
    private static func list() -> UICollectionView? {
        let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
        var found: [UICollectionView] = []
        func walk(_ view: UIView) {
            guard !view.isHidden, view.alpha > 0 else { return }
            if let list = view as? UICollectionView { found.append(list) }
            view.subviews.forEach(walk)
        }
        window.map(walk)
        return found.max { $0.bounds.width * $0.bounds.height < $1.bounds.width * $1.bounds.height }
    }

    /// The app's memory as the system counts it, in megabytes.
    private static func footprint() -> Double {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
        }
        return result == KERN_SUCCESS ? Double(info.phys_footprint) / 1_048_576 : -1
    }

    private static func report(_ fields: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: fields, options: [.sortedKeys]) else { return }
        let text = String(decoding: data, as: UTF8.self)
        print("PRS_BENCH \(text)")
        try? data.write(to: URL.documentsDirectory.appending(path: "prs-bench.json"))
    }
}

/// A display link asking for the screen's full rate, timing each frame.
@MainActor
private final class FrameClock: NSObject {
    struct Scroll {
        var frames = 0, hitches = 0
        var hitchTime: CFTimeInterval = 0, worst: CFTimeInterval = 0, median: CFTimeInterval = 0, seconds: CFTimeInterval = 0
        /// Milliseconds of hitch per second of scrolling, as Instruments counts it.
        var hitchRatio: Double { seconds > 0 ? hitchTime * 1000 / seconds : 0 }
    }

    private var tick: ((CADisplayLink) -> Bool)?
    private var done: CheckedContinuation<Void, Never>?

    /// The time of the first frame at which `ready` holds.
    func first(where ready: @escaping () -> Bool) async -> CFTimeInterval {
        var at: CFTimeInterval = 0
        await run { link in
            guard ready() else { return false }
            at = link.timestamp
            return true
        }
        return at
    }

    /// Scrolls `list` to its end and back `passes` times at a steady speed.
    func scroll(_ list: UICollectionView, passes: Int, pointsPerSecond: CGFloat) async -> Scroll {
        var result = Scroll()
        var intervals: [CFTimeInterval] = []
        var last: CFTimeInterval?
        var down = true, turns = 0
        let top = -list.adjustedContentInset.top
        await run { link in
            let expected = link.targetTimestamp - link.timestamp
            if let last {
                let interval = link.timestamp - last
                intervals.append(interval)
                result.seconds += interval
                result.worst = max(result.worst, interval)
                if interval > expected * 1.5 {
                    result.hitches += 1
                    result.hitchTime += interval - expected
                }
                let bottom = max(top, list.contentSize.height + list.adjustedContentInset.bottom - list.bounds.height)
                var y = list.contentOffset.y + (down ? 1 : -1) * pointsPerSecond * CGFloat(interval)
                if y >= bottom || y <= top {
                    y = min(max(y, top), bottom)
                    down.toggle()
                    turns += 1
                }
                list.contentOffset.y = y
            }
            last = link.timestamp
            return turns >= passes * 2
        }
        result.frames = intervals.count
        result.median = intervals.sorted()[safe: intervals.count / 2] ?? 0
        return result
    }

    private func run(_ tick: @escaping (CADisplayLink) -> Bool) async {
        self.tick = tick
        let link = CADisplayLink(target: self, selector: #selector(frame))
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 80, maximum: 120, preferred: 120)
        await withCheckedContinuation { done = $0; link.add(to: .main, forMode: .common) }
        link.invalidate()
    }

    @objc private func frame(_ link: CADisplayLink) {
        guard let tick, tick(link) else { return }
        self.tick = nil
        done?.resume()
        done = nil
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? { indices.contains(index) ? self[index] : nil }
}
#endif
