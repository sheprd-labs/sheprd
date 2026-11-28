import Foundation

/// What a new workspace or chat may start with (`GET /api/v2/new`): the
/// agents Sheprd can run, and the only folders a workspace may open in.
public struct NewOptions: Codable, Hashable, Sendable {
    public var agents: [AgentChoice]
    /// Folders workspaces already use, most useful first.
    public var recent: [String]
    /// Project roots, each with the folders in it.
    public var roots: [Root]

    public struct Root: Codable, Hashable, Sendable, Identifiable {
        public var path: String
        public var folders: [String]
        public var id: String { path }
    }

    /// Keeps the chosen agent when offered, otherwise selects the first
    /// available one. An empty list has no valid selection.
    public func agent(preferring id: String) -> AgentChoice? {
        agents.first { $0.id == id } ?? agents.first
    }

    /// Every folder offered, recent ones first, once each.
    public var folders: [String] {
        var seen = Set<String>()
        return (recent + roots.flatMap(\.folders)).filter { seen.insert($0).inserted }
    }
}

public struct AgentChoice: Codable, Hashable, Sendable, Identifiable {
    /// What requests name it by, such as `claude`.
    public var id: String
    public var name: String

    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

/// `GET /api/v2/new/sources?cwd=`: what a new worktree of the folder's
/// repository may start from besides a new branch. A computer too old to
/// say answers 404, and offers only a new branch.
public struct NewSources: Codable, Hashable, Sendable {
    /// Local branches no checkout has, most recently committed first.
    public var branches: [String]
    /// Checkouts of the repository no workspace has open.
    public var checkouts: [Checkout]
    /// Where its pull requests and issues are listed; `nil` when no
    /// `upstream` or `origin` remote names one.
    public var repository: Repository?

    public struct Checkout: Codable, Hashable, Sendable, Identifiable {
        public var path: String
        /// `nil` on a detached HEAD.
        public var branch: String?
        public var id: String { path }

        public init(path: String, branch: String? = nil) {
            self.path = path
            self.branch = branch
        }
    }

    public struct Repository: Codable, Hashable, Sendable {
        public var host: String
        public var owner: String
        public var repo: String

        public init(host: String, owner: String, repo: String) {
            self.host = host
            self.owner = owner
            self.repo = repo
        }
    }

    public init(branches: [String] = [], checkouts: [Checkout] = [], repository: Repository? = nil) {
        self.branches = branches
        self.checkouts = checkouts
        self.repository = repository
    }
}

/// `POST /api/v2/workspaces`: a workspace in `cwd`, on a worktree when
/// `branch` is not empty (a local branch is checked out as it is) or
/// `source` names one, with a chat running `agent` or a shell.
public struct NewWorkspaceRequest: Codable, Hashable, Sendable {
    public var cwd: String
    public var branch: String
    public var agent: String
    /// Sent once the chat is ready; empty sends nothing.
    public var prompt: String
    /// What the chat starts with, before the prompt goes.
    public var start: ChatStart?
    /// The Herdr session to open it in, as the computer's home names it;
    /// left out, it opens where the computer puts new workspaces.
    public var session: String?
    /// The workspace's name; left out, it is named for its folder or branch.
    public var title: String?
    /// Where the worktree comes from instead of `branch`.
    public var source: Source?
    /// What the first pane runs; left out, a chat.
    public var opens: Opens?

    /// A worktree the computer finds: a pull request of
    /// `NewSources.repository`, whose head it fetches, or a checkout
    /// `NewSources.checkouts` listed.
    public enum Source: Codable, Hashable, Sendable {
        case pull(number: Int, head: String, headOwner: String?)
        case checkout(path: String)

        private enum CodingKeys: String, CodingKey {
            case kind, number, head, path
            case headOwner = "head_owner"
        }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            switch try c.decode(String.self, forKey: .kind) {
            case "pull":
                self = try .pull(number: c.decode(Int.self, forKey: .number), head: c.decode(String.self, forKey: .head), headOwner: c.decodeIfPresent(String.self, forKey: .headOwner))
            case "checkout":
                self = try .checkout(path: c.decode(String.self, forKey: .path))
            default:
                throw DecodingError.dataCorruptedError(forKey: .kind, in: c, debugDescription: "unknown source")
            }
        }

        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case let .pull(number, head, headOwner):
                try c.encode("pull", forKey: .kind)
                try c.encode(number, forKey: .number)
                try c.encode(head, forKey: .head)
                try c.encodeIfPresent(headOwner, forKey: .headOwner)
            case let .checkout(path):
                try c.encode("checkout", forKey: .kind)
                try c.encode(path, forKey: .path)
            }
        }
    }

    /// What the new workspace's first pane runs, as the desktop's "Opens in".
    public enum Opens: String, Codable, Hashable, Sendable {
        case chat, shell
    }

    public init(cwd: String, branch: String = "", agent: String, prompt: String = "", start: ChatStart? = nil, session: String? = nil, title: String? = nil, source: Source? = nil, opens: Opens? = nil) {
        self.cwd = cwd
        self.branch = branch
        self.agent = agent
        self.prompt = prompt
        self.start = start
        self.session = session
        self.title = title
        self.source = source
        self.opens = opens
    }

    /// The branch proposed for `seed`, the same one the desktop's new
    /// worktree dialog proposes (`generated_branch_slug` in
    /// `sheprd-gpui`'s `worktree.rs`), so the form can be sent as it opens.
    public static func proposedBranch(seed: UInt64) -> String {
        let adjectives = ["brave", "calm", "clear", "green", "lucky", "quiet", "rapid", "silver"]
        let nouns = ["river", "cloud", "field", "forest", "harbor", "meadow", "stone", "valley"]
        let adjective = adjectives[Int(seed % UInt64(adjectives.count))]
        let noun = nouns[Int(seed / UInt64(adjectives.count) % UInt64(nouns.count))]
        let suffix = String(seed & 0xffff, radix: 16)
        return "worktree/\(adjective)-\(noun)-\(String(repeating: "0", count: 4 - suffix.count))\(suffix)"
    }

    /// A proposal seeded from the clock in microseconds, as the desktop's is.
    public static func proposedBranch(at date: Date = Date()) -> String {
        proposedBranch(seed: UInt64(max(date.timeIntervalSince1970, 0) * 1_000_000))
    }
}

/// `POST /api/v2/workspaces/{workspace}/chats`.
public struct NewChatRequest: Codable, Hashable, Sendable {
    public var agent: String
    public var prompt: String
    /// What the chat starts with, before the prompt goes.
    public var start: ChatStart?

    public init(agent: String, prompt: String = "", start: ChatStart? = nil) {
        self.agent = agent
        self.prompt = prompt
        self.start = start
    }
}

/// What creating answers: the chat's pane once its program reported in,
/// or `nil` when it was slow to start (it then appears on home later).
public struct Created: Codable, Hashable, Sendable {
    public var workspace: String?
    public var pane: String?
    /// Whether the first prompt went to the chat.
    public var prompted: Bool
    /// Whether the chat took what it was asked to start with; `nil` from a
    /// daemon too old to know the request's `start`, which ignored it.
    public var started: Bool?

    enum CodingKeys: String, CodingKey { case workspace, pane, prompted, started }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        workspace = try c.decodeIfPresent(String.self, forKey: .workspace)
        pane = try c.decodeIfPresent(String.self, forKey: .pane)
        prompted = try c.decodeIfPresent(Bool.self, forKey: .prompted) ?? false
        started = try c.decodeIfPresent(Bool.self, forKey: .started)
    }
}
