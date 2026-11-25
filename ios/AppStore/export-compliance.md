# Export compliance

The app's Info.plist declares `ITSAppUsesNonExemptEncryption = false`, so App
Store Connect asks nothing per build. The app's owner reviewed this inventory
and confirmed that declaration on 22 November 2026. Review it again when the
app adds cryptography. This is an inventory, not legal advice.

## What the app encrypts with

Every algorithm comes from Apple's CryptoKit and Security frameworks, which
are part of the operating system:

| Use | Algorithms (CryptoKit) |
| --- | --- |
| HTTPS and WebSockets to paired computers | The system's TLS (URLSession), with the computer's CA pinned |
| Pairing, session signatures, device keys | P-256 ECDSA and ECDH in the Secure Enclave, SHA-256 |
| Sealed alerts and Live Activities | P-256 or X25519 key agreement, HKDF-SHA256, AES-GCM |
| Relay tunnel (`SheprdTunnel`) | `Noise_IKpsk2_25519_ChaChaPoly_SHA256` plus ML-KEM-768, built from CryptoKit's X25519, ChaChaPoly, SHA-256, HMAC, HKDF, and MLKEM768 |

The tunnel combines standard primitives in a published protocol (Noise, with
ML-KEM as in hybrid post-quantum handshakes); it implements no algorithm of
its own.

The Rust fleet log library (`SheprdFleetFFI`, from `crates/sheprd-fleet-ffi`)
verifies signatures with Ed25519 and P-256 ECDSA (authentication only). Its
dependency graph also reaches `rustls` and `ring` through
`sheprd-remote-control`'s GitHub client; the library's functions do not call
them, and the linker should strip them, but this is worth confirming with
`nm` on a release archive, or by cutting that dependency with a feature.

## Apple's questions

1. Does the app use encryption? Yes.
2. Is it exempt? Apple's guidance treats an app that only uses the encryption
   in Apple's operating system, or uses standard algorithms for
   authentication, as exempt from uploading documentation. The relay tunnel
   encrypts user data end to end with OS-provided algorithms, which is the case
   to confirm.
3. If it is not exempt, the usual path is mass-market classification
   (ECCN 5D992.c) with no annual self-classification report since 2021, and
   an encryption declaration for distribution in France.
