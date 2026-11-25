import Foundation
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif

/// Where a relay is, read the way the Rust crate's `Endpoint::parse` reads
/// it, all normalized to `https://HOST[:PORT][/PREFIX/]`:
///
/// - `relay.example.org` or `relay.example.org:8443` (https implied)
/// - `https://relay.example.org/acme`, behind a proxy that strips the prefix
/// - `wss://relay.example.org`, the same as `https://`
/// - `http://` or `ws://` only for this computer, a private address, or a
///   `.local` name.
public struct RelayAddress: Hashable, Sendable, Codable, CustomStringConvertible {
    /// Why text is not a relay address.
    public enum Error: Swift.Error, Equatable, Sendable {
        // keep-sorted start
        /// A user name or password.
        case credentials
        /// A query or fragment.
        case extra
        /// Plain http to a host that is not local.
        case insecure
        /// Not a URL.
        case invalid
        /// No host.
        case noHost
        /// A scheme other than https, wss, http, or ws.
        case scheme(String)
        // keep-sorted end
    }

    /// Whether traffic to the relay is encrypted (https or wss).
    public let isSecure: Bool
    /// The host, lowercased; an IPv6 address without brackets.
    public let host: String
    /// The port, when it is not the scheme's default.
    public let port: Int?
    /// The path, starting and ending with `/`.
    let path: String

    public init(_ text: String) throws {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let full = trimmed.contains("://") ? trimmed : "https://" + trimmed
        guard let components = URLComponents(string: full), let scheme = components.scheme?.lowercased() else {
            throw Error.invalid
        }
        switch scheme {
        case "https", "wss": isSecure = true
        case "http", "ws": isSecure = false
        default: throw Error.scheme(scheme)
        }
        var host = (components.percentEncodedHost ?? "").lowercased()
        if host.hasPrefix("["), host.hasSuffix("]") { host = String(host.dropFirst().dropLast()) }
        guard !host.isEmpty else { throw Error.noHost }
        if let port = components.port, !(0...65_535).contains(port) { throw Error.invalid }
        guard isSecure || Self.isLocal(host) else { throw Error.insecure }
        guard components.user == nil, components.password == nil else { throw Error.credentials }
        guard components.query == nil, components.fragment == nil else { throw Error.extra }
        let defaultPort = isSecure ? 443 : 80
        self.host = host
        port = components.port.flatMap { $0 == defaultPort ? nil : $0 }
        var path = components.percentEncodedPath
        if !path.hasPrefix("/") { path = "/" + path }
        if !path.hasSuffix("/") { path += "/" }
        self.path = path
        guard makeURL(scheme: "https", "v1/info") != nil else { throw Error.invalid }
    }

    /// `host[:port]`, as the WebSocket's `Host` header carries it.
    public var authority: String {
        let shown = host.contains(":") ? "[\(host)]" : host
        return port.map { "\(shown):\($0)" } ?? shown
    }

    /// The name WebSocket proofs sign over: the authority, and the path
    /// prefix without its trailing slash when there is one
    /// (`relay.example.org/acme`).
    public var proofName: String {
        var prefix = Substring(path)
        while prefix.hasSuffix("/") { prefix = prefix.dropLast() }
        return authority + prefix
    }

    /// The base URL, without the trailing slash of an empty path.
    public var description: String {
        let base = "\(isSecure ? "https" : "http")://\(authority)\(path)"
        return base.hasSuffix("/") ? String(base.dropLast()) : base
    }

    /// `GET`, to check a relay before using it.
    public var info: URL { url(secure: isSecure ? "https" : "http", "v1/info") }

    /// A listener's control socket.
    public var listen: URL { socket("v1/listen") }

    /// The socket that dials `route` (32 bytes).
    public func dial(route: Data) -> URL { socket("v1/dial/\(Base64URL.encode(route))") }

    /// A listener's socket for one dial, `session` as the relay's
    /// `incoming` message gave it.
    public func accept(session: String) -> URL {
        let safe = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_")
        return socket("v1/accept/\(session.addingPercentEncoding(withAllowedCharacters: safe) ?? "")")
    }

    private func socket(_ rest: String) -> URL { url(secure: isSecure ? "wss" : "ws", rest) }

    private func url(secure scheme: String, _ rest: String) -> URL {
        // `init` checked that the parts make a URL; base64url adds nothing
        // a path cannot hold.
        makeURL(scheme: scheme, rest) ?? URL(fileURLWithPath: "/")
    }

    private func makeURL(scheme: String, _ rest: String) -> URL? {
        var components = URLComponents()
        components.scheme = scheme
        components.percentEncodedHost = host.contains(":") ? "[\(host)]" : host
        components.port = port
        components.percentEncodedPath = path + rest
        return components.url
    }

    public init(from decoder: Decoder) throws {
        let text = try decoder.singleValueContainer().decode(String.self)
        do {
            try self.init(text)
        } catch {
            throw DecodingError.dataCorrupted(.init(
                codingPath: decoder.codingPath, debugDescription: "not a relay address", underlyingError: error
            ))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(description)
    }

    /// Whether plain http may reach `host`: this computer, a `.local` name,
    /// or a loopback, private, link-local, or unique local address.
    static func isLocal(_ host: String) -> Bool {
        let name = host.hasSuffix(".") ? String(host.dropLast()) : host
        if name == "localhost" || name.hasSuffix(".localhost") || name.hasSuffix(".local") { return true }
        var v4 = in_addr()
        if inet_pton(AF_INET, name, &v4) == 1 {
            return isPrivate(withUnsafeBytes(of: &v4) { Array($0) })
        }
        var v6 = in6_addr()
        guard inet_pton(AF_INET6, name, &v6) == 1 else { return false }
        let bytes = withUnsafeBytes(of: &v6) { Array($0) }
        // IPv4-mapped (`::ffff:a.b.c.d`) reads as the IPv4 address.
        if bytes[0..<10].allSatisfy({ $0 == 0 }), bytes[10] == 0xff, bytes[11] == 0xff {
            return isPrivate(Array(bytes[12..<16]))
        }
        let loopback = bytes[0..<15].allSatisfy { $0 == 0 } && bytes[15] == 1
        let uniqueLocal = bytes[0] & 0xfe == 0xfc
        let linkLocal = bytes[0] == 0xfe && bytes[1] & 0xc0 == 0x80
        return loopback || uniqueLocal || linkLocal
    }

    private static func isPrivate(_ octets: [UInt8]) -> Bool {
        switch (octets[0], octets[1]) {
        case (127, _), (10, _), (192, 168), (169, 254): true
        case (172, 16...31): true
        default: false
        }
    }
}
