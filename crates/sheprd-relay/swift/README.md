# RelayClient

A Swift client for the relay's WebSocket protocol, for apps that dial a
listener through the relay from iOS 16, macOS 13, watchOS 9, or tvOS 16 and
later. It uses only Foundation and CryptoKit.

It covers:

- `RelayProof`: the signed bytes for listen and dial proofs, routes
  (SHA-256 of a SEC1 compressed P-256 key), the pairing dial key, and the
  hello JSON.
- `RelayAddress`: reads a relay address as the server's `Endpoint::parse`
  does (https implied, `wss` the same as `https`, plain `http`/`ws` only for
  local hosts) and builds its proof name and `info`, `listen`, `dial`, and
  `accept` URLs.
- `RelayStatus` and `RelayCloseCode`: the relay's answers and WebSocket
  close codes.
- `RelayFraming`: the optional message layer (a big-endian `UInt16` length,
  then the message) and a capped `Deframer`.
- `RelayDialer` and `RelayConnection`: the dial (challenge, hello, status)
  and the session after it.

There is no listener in Swift yet.

## Usage

An app called acme dials its desktop's route through `relay.example.org`:

```swift
import CryptoKit
import RelayClient

let address = try RelayAddress("relay.example.org/acme")
// The key the listener allows to dial it, kept in the app's keychain.
let dialKey = P256.Signing.PrivateKey()
// The listener's route, from pairing: SHA-256 of its public key.
let route: Data = acmePairing.route

let connection: RelayConnection
do {
    connection = try await RelayDialer(address: address).dial(route: route, key: dialKey)
} catch RelayDialError.refused(.offline) {
    // No one listens, or not for this key.
    return
}
// Other failures: `.refused(status)`, `.closed(code:)` with a
// `RelayCloseCode`, `.banned`, `.protocolViolation`, or a URLError.

try await connection.sendMessage(Data("hello".utf8))
while let message = try await connection.receiveMessage() {
    print(message)
}
await connection.close()
```

The relay passes session bytes untouched; put your own end-to-end
encryption inside them. It counts WebSocket pings as activity, so an idle
session stays open with `try await connection.ping()` every so often,
without any in-band keepalive.

## Tests

```sh
swift test
```

The tests read the server's vectors in place from `../vectors`, so the
Swift and Rust sides check the same bytes. Signatures there must verify;
CryptoKit randomizes ECDSA, so they are never compared byte for byte.
