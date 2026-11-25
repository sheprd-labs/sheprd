import Foundation

/// The demo's made-up computers and what they show (`DemoFleet` in the
/// app): homes, chats, and a terminal as the daemon would send them, so
/// they decode through the same types. No real computer, prompt, or output.
public enum DemoData {
    /// The address a demo computer is "online" at; never dialed.
    public static let origin = "demo"

    public static let studio = computer("demo-studio", name: "Studio")
    public static let laptop = computer("demo-laptop", name: "MacBook Air")
    public static let computers = [studio, laptop]

    /// Whether `device` is one of the demo's computers.
    public static func owns(_ device: String) -> Bool {
        computers.contains { $0.device == device }
    }

    /// What `device` sends as its home; nil for any other computer.
    public static func home(of device: String, now: Date = .now) -> Home? {
        let json = switch device {
        case studio.device: studioHome(now: now)
        case laptop.device: laptopHome
        default: ""
        }
        guard case let .home(home)? = try? JSONDecoder().decode(LiveEvent.self, from: Data(json.utf8)) else { return nil }
        return home
    }

    /// The transcript of a demo chat kept here; nil for the others.
    public static func transcript(pane: String, now: Date = .now) -> Transcript? {
        let json = switch pane {
        case "c2": questionChat(now: now)
        case "c5": laptopChat
        default: ""
        }
        return try? JSONDecoder().decode(ChatSnapshot.self, from: Data(json.utf8)).transcript
    }

    /// What a demo terminal shows; nil for any other pane.
    public static func screen(pane: String) -> TermScreen? {
        guard pane == "p3", case let .termScreen(screen)? = try? JSONDecoder().decode(LiveEvent.self, from: Data(terminal.utf8))
        else { return nil }
        return screen
    }

    private static func computer(_ device: String, name: String) -> Computer {
        // A fingerprint no certificate has, made from the name, so the
        // demo's computers are told apart and match no real one.
        Computer(device: device, name: name, origins: [], fingerprint: Fingerprint(certificate: Data(device.utf8)),
                 keyKind: .software, keyData: Data())
    }

    private static func ms(_ date: Date) -> UInt64 { UInt64(date.timeIntervalSince1970 * 1000) }

    private static let question = #"""
    {"call_id": "call-push", "id": 7, "title": "Run `git push origin fix/widget-refresh`?",
     "options": [{"id": "allow", "kind": "allow_once", "name": "Allow once"},
                 {"id": "always", "kind": "allow_always", "name": "Always allow"},
                 {"id": "reject", "kind": "reject_once", "name": "Deny"}]}
    """#

    private static func studioHome(now: Date) -> String {
        let started = ms(now.addingTimeInterval(-95))
        let ago = { (seconds: TimeInterval) in ms(now.addingTimeInterval(-seconds)) }
        return #"""
        {"type": "home", "home": {
          "api": 1, "computer": "Studio", "fingerprint": "\#(studio.fingerprint.hex)", "connected": true,
          "waiting": 1, "working": 2, "version": "demo",
          "load": {"cores": 12, "cpu": 38, "load5_tenths": 31, "memory": 54},
          "workspaces": [
            {"id": "w1", "label": "forecast", "labels": ["Widgets"], "branch": "main", "cwd": "/Users/demo/forecast", "session": "default",
             "worktree": {"key": "/Users/demo/forecast/.git", "label": "forecast", "is_linked_worktree": false},
             "pull_request": {"number": 412, "url": "https://example.com/forecast/pull/412", "tone": "waiting"},
             "chats": [
               {"pane": "c2", "tab": "t2", "title": "Ship the widget fix", "labels": ["Release"], "agent": "Claude", "status": "working",
                "summary": {"connected": true, "status": {"state": "working"}, "question": \#(question),
                            "questions": 1, "last_text": "Tests pass. Pushing the branch for review.",
                            "turn_started_ms": \#(started), "usage_percent": 31}},
               {"pane": "c1", "tab": "t1", "title": "Weather widget", "agent": "Claude", "status": "idle",
                "favorite": true, "unread": true,
                "summary": {"connected": true, "status": {"state": "idle"}, "questions": 0,
                            "last_text": "The widget now reloads after a background fetch."}}
             ],
             "terminals": [
               {"tab": "t3", "label": "zsh", "cwd": "/Users/demo/forecast", "zoomed": false, "actions": [],
                "panes": [{"pane": "p3", "label": null, "cwd": "/Users/demo/forecast", "focused": true}]}
             ]},
            {"id": "w4", "label": "chart legend", "labels": ["Widgets"], "branch": "feat/chart-legend", "cwd": "/Users/demo/forecast-legend",
             "session": "default", "uncommitted": true, "unopened_commits": 2,
             "worktree": {"key": "/Users/demo/forecast/.git", "label": "forecast", "is_linked_worktree": true},
             "chats": [
               {"pane": "c4", "tab": "t4", "title": "Show the chart legend", "agent": "Claude", "status": "working",
                "summary": {"connected": true, "status": {"state": "working"}, "questions": 0,
                            "last_text": "Two sub-agents are mapping the chart views.",
                            "turn_started_ms": \#(ago(184))}}
             ],
             "terminals": []}
          ]}}
        """#
    }

    private static let laptopHome = #"""
    {"type": "home", "home": {
      "api": 1, "computer": "MacBook Air", "fingerprint": "\#(laptop.fingerprint.hex)", "connected": true,
      "waiting": 0, "working": 0, "version": "demo",
      "load": {"cores": 8, "cpu": 12, "load5_tenths": 9, "memory": 41},
      "workspaces": [
        {"id": "w5", "label": "website", "branch": "main", "cwd": "/Users/demo/website", "session": "default",
         "worktree": {"key": "/Users/demo/website/.git", "label": "website", "is_linked_worktree": false},
         "chats": [
           {"pane": "c5", "tab": "t5", "title": "Landing page copy", "agent": "Codex", "status": "idle",
            "summary": {"connected": true, "status": {"state": "idle"}, "questions": 0,
                        "last_text": "Rewrote the hero and the three feature blurbs."}}
         ],
         "terminals": []}
      ]}}
    """#

    private static func questionChat(now: Date) -> String {
        #"""
        {"pane": "c2", "connected": true, "cursor": null, "transcript": {
          "agent": "Claude", "auto_accept": false, "clients": 1, "commands": [], "modes": null, "options": [],
          "plan": [], "queued": [], "read_only": false, "session": null, "truncated": false,
          "status": {"state": "working"}, "turn": {"started_ms": \#(ms(now.addingTimeInterval(-95)))},
          "usage": {"cost": null, "size": 200000, "used": 62000},
          "permissions": [\#(question)],
          "entries": [
            {"id": 1, "kind": {"type": "user", "text": "The widget fix looks good. Run the tests, then push the branch and open a pull request."}},
            {"id": 2, "kind": {"type": "agent", "text": "All **148 tests** pass, including the two new widget tests.\n\nI'll push `fix/widget-refresh` and open the pull request next."}}
          ]}}
        """#
    }

    private static let laptopChat = #"""
    {"pane": "c5", "connected": true, "cursor": null, "transcript": {
      "agent": "Codex", "auto_accept": false, "clients": 1, "commands": [], "modes": null, "options": [],
      "plan": [], "queued": [], "read_only": false, "session": null, "truncated": false,
      "status": {"state": "idle"}, "permissions": [],
      "usage": {"cost": null, "size": 200000, "used": 18000},
      "entries": [
        {"id": 1, "kind": {"type": "user", "text": "Make the landing page copy shorter and friendlier."}},
        {"id": 2, "kind": {"type": "agent", "text": "Rewrote the hero and the three feature blurbs:\n\n- **Hero:** \"Your forecast, at a glance.\"\n- Each blurb is now one sentence.\n- The call to action reads \"Get the app\"."}}
      ]}}
    """#

    private static let terminal = #"""
    {"type": "term_screen", "pane": "p3", "columns": 44, "rows": [
      [{"text": "~/forecast", "fg": "cyan"}, {"text": " "}, {"text": "❯", "fg": "green", "bold": true}, {"text": " swift test"}],
      [{"text": "Building for debugging...", "dim": true}],
      [{"text": "Build complete!", "fg": "green"}, {"text": " (4.12s)", "dim": true}],
      [{"text": "✔", "fg": "green"}, {"text": " ForecastTests (96 tests)"}],
      [{"text": "✔", "fg": "green"}, {"text": " ForecastWidgetTests (52 tests)"}],
      [{"text": "✔", "fg": "green", "bold": true}, {"text": " 148 tests passed", "bold": true}, {"text": " in 3.8s", "dim": true}],
      [{"text": ""}],
      [{"text": "~/forecast", "fg": "cyan"}, {"text": " "}, {"text": "❯", "fg": "green", "bold": true}, {"text": " "}]
    ]}
    """#
}
