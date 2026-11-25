import Foundation

/// An optional message layer for sessions: each message is its length, a
/// big-endian `UInt16`, then that many bytes. The relay never reads it;
/// peers that want messages agree on it among themselves. WebSocket frames
/// may split or join messages anywhere, so a reader keeps what it has until
/// a message is whole.
public enum RelayFraming {
    /// The bytes ahead of each message.
    public static let prefixLength = 2
    /// The longest message the prefix can describe.
    public static let maxMessage = Int(UInt16.max)

    /// Why bytes cannot be framed or read as messages.
    public enum Error: Swift.Error, Equatable, Sendable {
        // keep-sorted start
        /// A message's prefix says `length`, over the reader's `cap`.
        case overCap(cap: Int, length: Int)
        /// A message longer than ``RelayFraming/maxMessage``.
        case tooLong(length: Int)
        // keep-sorted end
    }

    /// `message`, framed: its length, then the message.
    public static func encode(_ message: Data) throws -> Data {
        guard let length = UInt16(exactly: message.count) else { throw Error.tooLong(length: message.count) }
        var framed = Data(capacity: prefixLength + message.count)
        framed.append(UInt8(length >> 8))
        framed.append(UInt8(length & 0xff))
        framed.append(contentsOf: message)
        return framed
    }

    /// Reads framed messages from bytes pushed in any pieces. It refuses a
    /// message longer than its cap as soon as the prefix says so, before
    /// waiting for its body, so it holds at most a message, its prefix, and
    /// the last piece pushed.
    public struct Deframer: Sendable {
        private var buffer = Data()
        /// The longest message this reader takes.
        public let cap: Int

        /// A reader for messages of at most `cap` bytes, itself at most
        /// ``RelayFraming/maxMessage``.
        public init(cap: Int = RelayFraming.maxMessage) {
            self.cap = max(0, min(cap, RelayFraming.maxMessage))
        }

        /// Bytes held toward the next message.
        public var buffered: Int { buffer.count }

        /// How many more bytes the next message needs: at least one when it
        /// is not whole.
        public var wanted: Int {
            guard let length else { return prefixLength - buffer.count }
            return max(prefixLength + length - buffer.count, 1)
        }

        /// Adds bytes that arrived, then gives the first whole message, if
        /// one is. Call ``next()`` until it gives `nil` to drain the rest.
        public mutating func push(_ bytes: Data) throws -> Data? {
            buffer.append(contentsOf: bytes)
            return try next()
        }

        /// The next whole message from what was pushed, if one is.
        public mutating func next() throws -> Data? {
            guard let length else { return nil }
            guard length <= cap else { throw Error.overCap(cap: cap, length: length) }
            let end = prefixLength + length
            guard buffer.count >= end else { return nil }
            let start = buffer.startIndex
            let message = Data(buffer[(start + prefixLength)..<(start + end)])
            buffer = Data(buffer[(start + end)...])
            return message
        }

        /// The next message's length, once its prefix is in.
        private var length: Int? {
            guard buffer.count >= prefixLength else { return nil }
            let start = buffer.startIndex
            return Int(buffer[start]) << 8 | Int(buffer[start + 1])
        }
    }
}
