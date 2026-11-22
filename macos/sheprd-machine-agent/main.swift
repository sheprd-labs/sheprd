// sheprd-machine-agent: Sheprd's agent inside a virtual Mac that Sheprd
// runs (crates/sheprd-gpui/src/machine/agent.rs). It runs as a user
// LaunchAgent in the guest's login session, so it can see the screen and
// post input there, and answers the host over vsock: no network, and
// nothing outside the virtual machine can reach it.
//
// Each connection carries one request and its reply, each a 4-byte
// big-endian length and then JSON:
//
//   {"op":"hello"}
//     -> {"ok":true,"version":1,"width":W,"height":H,"scale":S,
//         "accessibility":B,"screen_recording":B}
//   {"op":"exec","argv":["…"],"cwd":"~/x","timeout_ms":N}
//     -> {"ok":true,"status":N,"stdout":"…","stderr":"…","truncated":B,"timed_out":B}
//        ("signal":N in place of "status" when a signal ended it; each
//        stream's end, at most 1 MiB)
//   {"op":"screenshot"}       -> {"ok":true,"png":"base64","width":W,"height":H}
//   {"op":"click","x":X,"y":Y,"button":"left|right","count":N}  -> {"ok":true}
//   {"op":"type","text":"…"}  -> {"ok":true}
//   {"op":"key","key":"return","modifiers":["cmd","shift","option","control"]}
//                             -> {"ok":true}
//
// A refusal is {"ok":false,"error":"accessibility|screen_recording|invalid|failed",
// "message":"…"}. Points are screenshot pixels, top-left origin.
//
// Kept in step with the host's copy of this contract (agent/wire.rs).

import ApplicationServices
import CoreGraphics
import Foundation

/// The vsock port the host connects to.
let agentPort: UInt32 = 7470
/// The contract's version, which `hello` reports.
let agentVersion = 1

setvbuf(stdout, nil, _IOLBF, 0)
// A fresh copy asked what macOS allows now (Permissions.swift).
if CommandLine.arguments.contains("--permissions") {
    print(Permissions.own().line)
    exit(0)
}
askForPermissions()
listen(port: agentPort)

/// Shows macOS's own prompts for what input and screenshots need, once:
/// macOS remembers the answer for this app.
func askForPermissions() {
    askForAccessibility()
    if !CGPreflightScreenCaptureAccess() {
        _ = CGRequestScreenCaptureAccess()
    }
}

/// Shows the Accessibility prompt unless it was answered. The key is
/// `kAXTrustedCheckOptionPrompt`'s value, which Swift 6 will not read from
/// a global.
func askForAccessibility() {
    _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
}
