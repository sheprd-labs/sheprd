import Foundation

/// The changing part of a chat's Live Activity: `ChatActivityAttributes`'s
/// `ContentState`. The daemon writes the same keys as `content-state` in
/// its APNs payloads (`crates/sheprd-remote-control/src/apns/payload.rs`), and the
/// fixtures in `Tests/SheprdRemoteCoreTests/Fixtures` hold both sides to it:
///
/// | Key | Type | |
/// | --- | --- | --- |
/// | `phase` | `"working"`, `"question"`, `"finished"`, `"failed"` | |
/// | `step` | string | current tool call or plan step |
/// | `startedAt` | number | turn start, seconds since 1970 |
/// | `question` | string? | the question waiting |
/// | `summary` | string? | how the turn ended |
/// | `planDone`, `planTotal` | int? | plan progress |
/// | `working`, `waiting` | int, default 0 | this computer's running turns and waiting questions |
/// | `permission` | int? | the waiting permission's id, in the question phase |
/// | `options` | `[{id, name, kind}]`, default empty | its first three options, for answer buttons |
/// | `previous` | `[string]`, default empty | the two steps before `step`, oldest first |
/// | `activity` | string? | `step` in plain words, for the Standard and Complete styles |
/// | `icon` | string? | the kind of step (`test`, `git`, `edit`, …), for its symbol |
/// | `previousActivities` | `[string]`, default empty | `previous` in plain words |
/// | `steps`, `files` | int? | the turn's tool calls and edited files so far |
///
/// Older daemons also sent `cost`, which is ignored: the price of a turn
/// is not worth the room on a phone.
///
/// `startedAt` is decoded here as Unix seconds rather than through a
/// `JSONDecoder` date strategy, which ActivityKit does not let an app pick.
public struct ChatActivityState: Codable, Hashable, Sendable {
    public enum Phase: String, Codable, Hashable, Sendable {
        case working
        case question
        case finished
        case failed
    }

    public struct AgentProgress: Codable, Hashable, Sendable {
        public var done: Int
        public var total: Int
        public var latest: String?

        public var fraction: Double { total > 0 ? min(1, max(0, Double(done) / Double(total))) : 0 }
        public var label: String { "\(done) of \(total) agents done" }
    }

    public var focus: ChatActivityFacts?
    public var agents: AgentProgress?
    public var phase: Phase
    public var step: String
    public var startedAt: Date
    public var question: String?
    public var summary: String?
    public var planDone: Int?
    public var planTotal: Int?
    public var working: Int
    public var waiting: Int
    public var permission: PermissionId?
    public var options: [Option]
    public var previous: [String]
    public var activity: String?
    public var icon: ActivityIcon?
    public var previousActivities: [String]
    public var steps: Int?
    public var files: Int?

    /// An answer the lock screen offers as a button.
    public struct Option: Codable, Hashable, Sendable {
        public var id: String
        public var name: String
        public var kind: PermissionKind

        public init(id: String, name: String, kind: PermissionKind) {
            self.id = id
            self.name = name
            self.kind = kind
        }
    }

    public init(
        phase: Phase,
        step: String,
        startedAt: Date,
        question: String? = nil,
        summary: String? = nil,
        planDone: Int? = nil,
        planTotal: Int? = nil,
        working: Int = 0,
        waiting: Int = 0,
        permission: PermissionId? = nil,
        options: [Option] = [],
        previous: [String] = [],
        activity: String? = nil,
        icon: ActivityIcon? = nil,
        previousActivities: [String] = [],
        steps: Int? = nil,
        files: Int? = nil
    ) {
        self.phase = phase
        self.step = step
        self.startedAt = startedAt
        self.question = question
        self.summary = summary
        self.planDone = planDone
        self.planTotal = planTotal
        self.working = working
        self.waiting = waiting
        self.permission = permission
        self.options = options
        self.previous = previous
        self.activity = activity
        self.icon = icon
        self.previousActivities = previousActivities
        self.steps = steps
        self.files = files
    }

    enum CodingKeys: String, CodingKey {
        case phase, step, startedAt, question, summary, planDone, planTotal, working, waiting
        case permission, options, previous
        case activity, icon, previousActivities, steps, files, agents, focus
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        focus = try container.decodeIfPresent(ChatActivityFacts.self, forKey: .focus)
        agents = try container.decodeIfPresent(AgentProgress.self, forKey: .agents)
        phase = try container.decode(Phase.self, forKey: .phase)
        step = try container.decode(String.self, forKey: .step)
        let seconds = try container.decode(Double.self, forKey: .startedAt)
        startedAt = Date(timeIntervalSince1970: seconds)
        question = try container.decodeIfPresent(String.self, forKey: .question)
        summary = try container.decodeIfPresent(String.self, forKey: .summary)
        planDone = try container.decodeIfPresent(Int.self, forKey: .planDone)
        planTotal = try container.decodeIfPresent(Int.self, forKey: .planTotal)
        working = try container.decodeIfPresent(Int.self, forKey: .working) ?? 0
        waiting = try container.decodeIfPresent(Int.self, forKey: .waiting) ?? 0
        permission = try container.decodeIfPresent(PermissionId.self, forKey: .permission)
        options = try container.decodeIfPresent([Option].self, forKey: .options) ?? []
        previous = try container.decodeIfPresent([String].self, forKey: .previous) ?? []
        activity = try container.decodeIfPresent(String.self, forKey: .activity)
        // A kind this app does not know draws no symbol rather than failing.
        icon = (try? container.decodeIfPresent(String.self, forKey: .icon)).flatMap(ActivityIcon.init(rawValue:))
        previousActivities = try container.decodeIfPresent([String].self, forKey: .previousActivities) ?? []
        steps = try container.decodeIfPresent(Int.self, forKey: .steps)
        files = try container.decodeIfPresent(Int.self, forKey: .files)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encodeIfPresent(focus, forKey: .focus)
        try container.encodeIfPresent(agents, forKey: .agents)
        try container.encode(phase, forKey: .phase)
        try container.encode(step, forKey: .step)
        try container.encode(startedAt.timeIntervalSince1970.rounded(.down), forKey: .startedAt)
        try container.encodeIfPresent(question, forKey: .question)
        try container.encodeIfPresent(summary, forKey: .summary)
        try container.encodeIfPresent(planDone, forKey: .planDone)
        try container.encodeIfPresent(planTotal, forKey: .planTotal)
        try container.encode(working, forKey: .working)
        try container.encode(waiting, forKey: .waiting)
        try container.encodeIfPresent(permission, forKey: .permission)
        if !options.isEmpty { try container.encode(options, forKey: .options) }
        if !previous.isEmpty { try container.encode(previous, forKey: .previous) }
        try container.encodeIfPresent(activity, forKey: .activity)
        try container.encodeIfPresent(icon, forKey: .icon)
        if !previousActivities.isEmpty { try container.encode(previousActivities, forKey: .previousActivities) }
        try container.encodeIfPresent(steps, forKey: .steps)
        try container.encodeIfPresent(files, forKey: .files)
    }

    /// The step in plain words: `activity`, or `step` from a computer that
    /// does not say it in words.
    public var plainStep: String { activity ?? step }

    /// The steps before it in plain words, falling back the same way.
    public var plainPrevious: [String] {
        previousActivities.count == previous.count ? previousActivities : previous
    }

    /// `14 steps · 3 files`, from what the computer counted.
    public var counts: String? {
        let parts = [
            steps.map { $0 == 1 ? "1 step" : "\($0) steps" },
            files.map { $0 == 1 ? "1 file" : "\($0) files" },
        ].compactMap(\.self)
        return parts.isEmpty ? nil : parts.joined(separator: " \u{b7} ")
    }

    /// Plan progress from 0 to 1, when the agent keeps a plan.
    public var planFraction: Double? {
        guard let done = planDone, let total = planTotal, total > 0 else { return nil }
        return min(1, max(0, Double(done) / Double(total)))
    }

    /// `Step 3 of 4` while the plan has a step in progress.
    public var planLabel: String? {
        guard let done = planDone, let total = planTotal, total > 0 else { return nil }
        return "Step \(min(done + 1, total)) of \(total)"
    }

    /// The line under the activity's title in each phase.
    public var headline: String {
        switch phase {
        case .working: step
        case .question: "Needs an answer"
        case .finished: "Finished"
        case .failed: "Stopped"
        }
    }
}

/// The fixed part of a chat's Live Activity, as plain data, so code outside
/// ActivityKit can name it. Its keys are `attributes` in a push-to-start
/// payload: `agent`, `chatTitle`, `computer`, `computerId`, `pane`,
/// `workspace`.
public struct ChatActivityFacts: Codable, Hashable, Sendable {
    /// The computer's name: display only, since any computer can pick any
    /// name.
    public var computer: String
    /// The computer's CA fingerprint as `Fingerprint.hex` writes it: 64
    /// uppercase hex digits, no separators. Links and push tokens are routed
    /// by it alone.
    public var computerId: String
    public var workspace: String
    public var chatTitle: String
    public var agent: String
    public var pane: String
    /// Set for an agent in a plain terminal, whose terminal the activity
    /// opens; absent for a chat.
    public var terminal: Bool?

    /// Whether this activity says which chat it is. A generic one has no
    /// title and shows the computer as its workspace, as the daemon's
    /// `Live::redacted` leaves it, whichever side asked for it.
    public var namesChat: Bool { !chatTitle.isEmpty || workspace != computer }

    public init(
        computer: String,
        computerId: String,
        workspace: String,
        chatTitle: String,
        agent: String,
        pane: String
    ) {
        self.computer = computer
        self.computerId = computerId
        self.workspace = workspace
        self.chatTitle = chatTitle
        self.agent = agent
        self.pane = pane
    }

    /// The computer the activity says it belongs to, when `computerId` is a
    /// fingerprint at all.
    public var fingerprint: Fingerprint? {
        Fingerprint(computerId)
    }

    /// The link the activity opens.
    public var link: ChatLink {
        ChatLink(computer: computer, computerId: computerId, pane: pane, terminal: terminal ?? false)
    }

    /// `workspace · chat`, or the workspace alone for an untitled chat.
    public var place: String {
        chatTitle.isEmpty ? workspace : "\(workspace) \u{b7} \(chatTitle)"
    }
}
