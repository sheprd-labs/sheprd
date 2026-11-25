import Foundation
import SheprdRemoteCore

/// A bounded local record. Never retains response bodies, headers, or error
/// userInfo (which can contain credentials and redirected URLs).
public struct RelayDiagnostic: Codable, Equatable, Sendable {
    public let endpoint: RelayEndpoint
    public let date: Date
    public let milliseconds: Int
    public let status: Int?
    public let outcome: Outcome

    public enum Outcome: Codable, Equatable, Sendable {
        case reachable, noWebSocket, invalidAddress, nonHTTP, oversized, invalidJSON
        case banned, httpError
        case protocolVersion(Int)
        case network(Int)
        case transportFailure
    }

    public var reach: RelayReach { outcome.reach }

    public var detail: String {
        switch outcome {
        case .reachable: "Relay info reachable (WebSocket connection not tested)"
        case .noWebSocket: "Relay does not offer WebSocket connections"
        case .invalidAddress: "Invalid relay address"
        case .nonHTTP: "Non-HTTP response"
        case .oversized: "Response exceeds 16 KiB"
        case .invalidJSON: "Response is not valid relay info JSON"
        case .banned: "This relay has banned your network address"
        case .httpError: "HTTP \(status ?? 0) from relay or proxy"
        case .protocolVersion(let version): "Unsupported relay protocol \(version)"
        case .network(let code): "\(URLError.Code(rawValue: code).diagnosticName) (URL error \(code))"
        case .transportFailure: "Transport failed"
        }
    }

    public var report: String {
        "\(date.ISO8601Format()) GET \(endpoint.info?.absoluteString ?? endpoint.description)\n\(detail); HTTP \(status.map(String.init) ?? "none"); \(milliseconds) ms"
    }
}

extension RelayDiagnostic.Outcome {
    var reach: RelayReach {
        switch self {
        case .reachable: .reachable
        case .banned: .banned
        case .noWebSocket: .noWebSocket
        case .network, .transportFailure, .httpError, .nonHTTP: .unreachable
        default: .notARelay
        }
    }

}

private extension URLError.Code {
    var diagnosticName: String {
        switch self {
        case .timedOut: "Timed out"
        case .cannotFindHost, .dnsLookupFailed: "DNS lookup failed"
        case .cannotConnectToHost: "Connection refused or unavailable"
        case .networkConnectionLost: "Network connection lost"
        case .notConnectedToInternet: "No internet connection"
        case .secureConnectionFailed: "TLS connection failed"
        case .serverCertificateHasBadDate, .serverCertificateUntrusted,
             .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid: "TLS certificate rejected"
        case .cancelled: "Check cancelled"
        default: "Network request failed"
        }
    }
}

extension RelayReach {
    static func outcome(status: Int, body: Data) -> RelayDiagnostic.Outcome {
        guard status == 200 else { return .httpError }
        guard let info = try? JSONDecoder().decode(Info.self, from: body) else { return .invalidJSON }
        guard info.protocol == 1 else { return .protocolVersion(info.protocol) }
        return info.websocket == false ? .noWebSocket : .reachable
    }

    public static func diagnose(_ endpoint: RelayEndpoint, session: URLSession = .shared) async -> RelayDiagnostic {
        let date = Date()
        let start = ContinuousClock.now
        var status: Int?
        func result(_ outcome: RelayDiagnostic.Outcome) -> RelayDiagnostic {
            let elapsed = start.duration(to: .now).components
            let milliseconds = Int(elapsed.seconds * 1_000 + elapsed.attoseconds / 1_000_000_000_000_000)
            return RelayDiagnostic(endpoint: endpoint, date: date, milliseconds: milliseconds, status: status, outcome: outcome)
        }
        guard let url = endpoint.info else { return result(.invalidAddress) }
        let request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 10)
        do {
            let (bytes, response) = try await session.bytes(for: request)
            defer { bytes.task.cancel() }
            guard let http = response as? HTTPURLResponse else { return result(.nonHTTP) }
            status = http.statusCode
            if http.statusCode == 403, http.value(forHTTPHeaderField: "sheprd-relay-banned") == "1" {
                return result(.banned)
            }
            guard http.statusCode == 200 else { return result(.httpError) }
            guard http.expectedContentLength <= Int64(maxInfo),
                  let body = try await bounded(bytes, limit: maxInfo) else { return result(.oversized) }
            return result(outcome(status: http.statusCode, body: body))
        } catch let error as URLError {
            return result(.network(error.code.rawValue))
        } catch {
            return result(.transportFailure)
        }
    }
}

/// Retain failures across recovery and app launches, with a fixed storage cap.
public struct RelayDiagnosticHistory: Codable, Sendable {
    public private(set) var entries: [RelayDiagnostic] = []
    public init() {}

    public mutating func append(_ entry: RelayDiagnostic) {
        entries.append(entry)
        entries = Array(entries.suffix(64))
    }

    public var report: String { entries.map(\.report).joined(separator: "\n\n") }
}
