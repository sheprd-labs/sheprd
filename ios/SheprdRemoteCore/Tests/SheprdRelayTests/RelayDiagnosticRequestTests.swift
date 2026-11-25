import Foundation
import Network
import SheprdRemoteCore
@testable import SheprdRelay
import Testing

@Suite(.timeLimit(.minutes(1)))
struct RelayDiagnosticRequestTests {
    @Test func anHTTPFailurePreservesTheStatusAndRequestDetails() async throws {
        let listener = try await server(response: "503 Service Unavailable")
        defer { listener.cancel() }
        let port = try #require(listener.port)
        let endpoint = try #require(RelayEndpoint("http://127.0.0.1:\(port.rawValue)"))
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        let before = Date()
        let diagnostic = await RelayReach.diagnose(endpoint, session: session)
        #expect(diagnostic.endpoint == endpoint)
        #expect(diagnostic.date >= before && diagnostic.date <= Date())
        #expect(diagnostic.milliseconds >= 0)
        #expect(diagnostic.status == 503)
        #expect(diagnostic.outcome == .httpError)
        #expect(diagnostic.reach == .unreachable)
        #expect(diagnostic.report.contains("HTTP 503"))
    }

    @Test(arguments: [true, false])
    func onlyAnExplicitAddressBanIsShownAsBanned(marked: Bool) async throws {
        let listener = try await server(response: marked ? "403 Forbidden\r\nSheprd-Relay-Banned: 1" : "403 Forbidden")
        defer { listener.cancel() }
        let port = try #require(listener.port)
        let endpoint = try #require(RelayEndpoint("http://127.0.0.1:\(port.rawValue)"))
        let diagnostic = await RelayReach.diagnose(endpoint)
        #expect(diagnostic.reach == (marked ? .banned : .unreachable))
        #expect(diagnostic.detail.contains("banned") == marked)
    }

    @Test func aTimedOutRequestPreservesTheURLErrorCode() async throws {
        let listener = try await server(response: nil)
        defer { listener.cancel() }
        let port = try #require(listener.port)
        let endpoint = try #require(RelayEndpoint("http://127.0.0.1:\(port.rawValue)"))
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForResource = 0.25
        let session = URLSession(configuration: configuration)
        defer { session.invalidateAndCancel() }
        let diagnostic = await RelayReach.diagnose(endpoint, session: session)
        #expect(diagnostic.endpoint == endpoint)
        #expect(diagnostic.status == nil)
        #expect(diagnostic.outcome == .network(URLError.timedOut.rawValue))
        #expect(diagnostic.reach == .unreachable)
        #expect(diagnostic.report.contains("URL error \(URLError.timedOut.rawValue)"))
    }

    /// A real local HTTP peer, either responding or keeping the request
    /// open until URLSession's resource deadline expires.
    private func server(response: String?) async throws -> NWListener {
        let parameters = NWParameters.tcp
        parameters.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: .any)
        let listener = try NWListener(using: parameters)
        listener.newConnectionHandler = { connection in
            connection.start(queue: .global())
            connection.receive(minimumIncompleteLength: 1, maximumLength: 4096) { _, _, _, _ in
                guard let response else {
                    connection.receive(minimumIncompleteLength: 1, maximumLength: 1) { _, _, _, _ in
                        connection.cancel()
                    }
                    return
                }
                let bytes = Data("HTTP/1.1 \(response)\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".utf8)
                connection.send(content: bytes, completion: .contentProcessed { _ in connection.cancel() })
            }
        }
        try await withCheckedThrowingContinuation { (ready: CheckedContinuation<Void, Error>) in
            listener.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    listener.stateUpdateHandler = nil
                    ready.resume()
                case .failed(let error):
                    listener.stateUpdateHandler = nil
                    ready.resume(throwing: error)
                default: break
                }
            }
            listener.start(queue: .global())
        }
        return listener
    }
}
