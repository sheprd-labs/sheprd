import Foundation
import SheprdRemoteCore
@testable import SheprdRelay
import Testing

struct RelayDiagnosticTests {
    @Test func responseFailuresKeepTheirCause() {
        #expect(RelayReach.outcome(status: 503, body: Data()) == .httpError)
        #expect(RelayReach.outcome(status: 502, body: Data("secret proxy page".utf8)) == .httpError)
        #expect(RelayReach.outcome(status: 200, body: Data("<html>".utf8)) == .invalidJSON)
        #expect(RelayReach.outcome(status: 200, body: Data(#"{"protocol":7}"#.utf8)) == .protocolVersion(7))
        #expect(RelayReach.outcome(status: 200, body: Data(#"{"protocol":1}"#.utf8)) == .reachable)
        #expect(RelayReach.outcome(status: 200, body: Data(#"{"protocol":1,"websocket":false}"#.utf8)) == .noWebSocket)
    }

    @Test func historyKeepsFailuresAfterRecoveryAndRoundTrips() throws {
        var history = RelayDiagnosticHistory()
        history.append(entry(.network(URLError.timedOut.rawValue)))
        history.append(entry(.reachable, status: 200))
        let saved = try JSONEncoder().encode(history)
        let restored = try JSONDecoder().decode(RelayDiagnosticHistory.self, from: saved)
        #expect(restored.entries == history.entries)
        #expect(restored.report.contains("Timed out"))
        #expect(restored.report.contains("HTTP 200"))
        #expect(restored.report.contains("123 ms"))
    }

    @Test func historyEvictsOldestChecks() {
        var history = RelayDiagnosticHistory()
        history.append(entry(.network(URLError.timedOut.rawValue)))
        for _ in 0..<64 { history.append(entry(.reachable)) }
        #expect(history.entries.count == 64)
        #expect(history.entries.allSatisfy { $0.outcome == .reachable })
    }

    @Test func serverErrorsAreNotCalledNotARelay() {
        let failure = entry(.httpError, status: 503)
        #expect(failure.reach == .unreachable)
        #expect(failure.detail == "HTTP 503 from relay or proxy")
        #expect(entry(.protocolVersion(2)).reach == .notARelay)
        #expect(entry(.network(URLError.serverCertificateUntrusted.rawValue)).detail.contains("TLS certificate rejected"))
    }

    private func entry(_ outcome: RelayDiagnostic.Outcome, status: Int? = nil) -> RelayDiagnostic {
        RelayDiagnostic(endpoint: RelayEndpoint("https://relay.example.org")!, date: Date(timeIntervalSince1970: 0),
                        milliseconds: 123, status: status, outcome: outcome)
    }
}
