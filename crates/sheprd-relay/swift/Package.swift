// swift-tools-version: 6.0
// A Swift client for the relay's WebSocket protocol: proofs, addresses,
// statuses, the optional message framing, and a dialer. Foundation and
// CryptoKit only. The tests read the Rust crate's vectors in `../vectors`.

import PackageDescription

let package = Package(
    name: "RelayClient",
    platforms: [.iOS(.v16), .macOS(.v13), .watchOS(.v9), .tvOS(.v16)],
    products: [
        .library(name: "RelayClient", targets: ["RelayClient"]),
    ],
    targets: [
        .target(name: "RelayClient"),
        .testTarget(name: "RelayClientTests", dependencies: ["RelayClient"]),
    ]
)
