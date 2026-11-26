# Linux desktop VMs with Wayland

**Status:** Phase 1 (feasibility spike) and Phase 2 (shared model and Apple backend boundary) complete; ready for Phase 3 (Linux provisioning and lifecycle).

Run a Linux desktop VM in a Sheprd tab, initially on an Apple silicon Mac,
with a native graphical display and a Wayland session inside the guest. Keep
the design ready for Windows guests and other VM backends without building
those implementations speculatively.

## Goals

- Create, start, stop, and reopen a Linux desktop VM from Sheprd.
- Show its live desktop in a native Sheprd tab; send pointer and keyboard
  input; capture screenshots; run guest commands.
- Reuse machine claims, image management, remote-host forwarding, and remote
  watching where their contracts fit.
- Keep macOS machines and existing machine data working unchanged.
- Put real operating-system and backend variation behind narrow traits so a
  future Windows implementation can reuse machine management, CLI, claims,
  and UI flows.

## Not in scope for the first release

- Running VMs on Linux or Windows host computers.
- A general-purpose hypervisor framework or a Windows backend.
- Supporting every Linux distribution or desktop environment.
- Guaranteeing accelerated OpenGL/Vulkan or gaming performance. A usable
  desktop and input path do not prove GPU acceleration.
- Sharing the host's Wayland compositor with guest applications. The Linux
  desktop runs in the VM; Sheprd displays the VM's screen.

“Windows later” has two independent meanings: a Windows guest on a supported
host, and Sheprd running a VM backend on a Windows host. Keep both possible,
but evaluate their actual platform, firmware, drivers, and display support
before promising either.

## Current implementation

- `crates/sheprd-gpui/src/machine/vm.rs` owns Virtualization.framework
  machines and their lifecycle in main-thread-local state. The framework's
  native `VZVirtualMachineView` displays the guest in the same process.
- `machine/configuration.rs` builds a macOS-specific configuration, and
  `machine/store.rs` assumes files such as `auxiliary.bin`, `hardware.bin`,
  and `state.vzsave`.
- `machine/agent.rs` connects to the macOS guest agent over vsock;
  `machine/agent/wire.rs` defines bounded framed JSON requests for execution,
  screenshots, and input. `macos/sheprd-machine-agent` implements the guest
  side using macOS APIs.
- `machine/quit.rs` expects running VMs to save state before app exit, while
  configuration validation requires save/restore support.
- `browser/machine_tab.rs`, machine claims, and remote-machine code already
  supply much of the tab, reservation, and remote operation flow.
- Machine labels and limits include macOS-specific wording and assumptions,
  including `MAX_ACTIVE_MACHINES` in
  `crates/sheprd-control-protocol/src/protocol/tabs.rs`.

These are constraints to resolve, not reasons to duplicate the feature in a
separate Linux-only product path.

## Architecture and trait boundaries

Separate the guest OS from the host VM backend. Linux is a guest definition;
Apple Virtualization.framework is the initial host backend. They are not the
same axis, and adding a Linux guest must not hard-code Apple framework types
into shared machine state or protocol types.

Use traits for concrete substitution points, not as wrappers around every
existing struct. Keep domain types, storage policy, and UI policy shared where
they do not vary.

| Boundary | Contract and placement | Why it varies |
| --- | --- | --- |
| `VmBackend` | Host-side capability discovery, configuration validation, and VM creation. Implement the Apple backend first; keep framework types inside its implementation. | A future Windows host or alternate hypervisor has different host capabilities and APIs. |
| `VmInstance` | Lifecycle operations and reported state/capabilities for one live VM. Keep instances in the existing main-thread-local ownership model when the backend requires it. | Suspend, resume, shutdown, and callbacks differ by backend and guest combination. |
| `GuestProvisioner` | Prepare a guest disk and boot artifacts, report provisioning progress, and finalize a clone's OS identity. Keep disk work off the UI thread. | Linux installation/cloud-init and a future Windows installation/generalization flow differ. |
| `GuestAgent` | OS-neutral host operations: readiness/capabilities, command execution, screen capture, and input. Keep transport (vsock initially) separate from the operation contract. | Guest session APIs and transports vary; the UI and CLI should not depend on Swift or a particular Linux desktop. |
| `MachinePresentation` | GPUI-side attachment, rendering, and sizing of a machine's desktop. It must not be part of the backend's platform-neutral domain API. | Apple can embed a native VM view; another backend may deliver frames or a remote-display surface. |

The table names the boundaries to validate, not a requirement to add five
traits before they have real implementations. Start with the narrowest trait
needed by at least two concrete behaviors. Use static dispatch for known
single-implementation paths and trait objects only where different
implementations must coexist at runtime. Do not blanket-require `Send + Sync`:
Apple VM objects and views have main-thread constraints. Disk preparation and
other blocking I/O should still run on workers.

### Shared domain model

- Represent guest OS and architecture as typed values, not strings or
  backend-specific mirror enums.
- Persist guest type, image identity/version, and versioned backend-specific
  boot metadata separately. Existing folders lacking guest metadata continue
  to mean macOS.
- Keep machine names, claims, common settings, CLI operations, and remote
  forwarding shared. Use backend/guest capabilities to determine what an
  operation can do.
- Make unsupported lifecycle features explicit. In particular, do not report
  a stopped Linux guest as suspended when it could not save state.
- Keep Apple types, unsafe framework calls, and main-thread proof inside the
  Apple backend and its GPUI presentation adapter.

### Agent contract

Keep the framed, bounded protocol compatible with current macOS agents. Add
explicit capabilities for command execution, screen capture, pointer input,
keyboard input, text input, and desktop-session readiness. Preserve the
existing macOS permission fields while migrating callers to the common
capabilities model; do not reinterpret `accessibility` or
`screen_recording` as portable concepts.

Specify portable key names, modifier release behavior, text/Unicode handling,
coordinate units, image format/size bounds, and readiness semantics. The host
side should talk to a `GuestAgent` contract; transport connection details
remain behind a connector so a later backend can use a different channel
without changing CLI or tab behavior.

## Linux desktop and provisioning

Begin with one pinned ARM64 Ubuntu LTS image and a lightweight Wayland
compositor/session (evaluate Sway or another wlroots-based option in the
spike). Include XWayland for legacy applications. Do not claim arbitrary
GNOME/KDE support until their unattended screen-capture and input paths have
been proven.

The image should provide a dedicated development user, terminal, browser,
fonts, basic tools, and a guest agent started in the logged-in graphical
session. The agent must receive the correct Wayland socket/session environment;
it must not run as an unrelated system service and assume access to the
desktop.

Linux configuration should investigate and validate the platform's generic
firmware/EFI, persistent variable store, virtio disk/network/graphics, input,
entropy, serial diagnostics, and vsock support. Start with one fixed display
mode. Add resize-to-tab only after resolution changes and pointer coordinate
mapping are reliable.

Wayland intentionally restricts global capture and input. For the controlled
image, use compositor-supported capture and virtual-input mechanisms. For
other desktops, evaluate portals/PipeWire and remote-desktop input separately;
unattended permission behavior is a release gate. Test the actual renderer:
virtio graphics and a visible desktop do not imply accelerated 3D.

Verify downloaded images by checksum. Show provisioning stages (download,
disk preparation, boot, configuration, desktop ready) and keep long disk work
off the UI thread. Publish a machine only after its files are complete. Image
clones must get independent guest identities; do not duplicate Linux machine
IDs, SSH host keys, or other per-machine secrets.

## Lifecycle, capacity, and compatibility

### Lifecycle

In the feasibility spike, test Linux save/restore using the intended display,
devices, and guest workload. Do not infer support from macOS VM behavior.
Report capabilities per VM configuration. If save/restore is unavailable,
gracefully shut down Linux guests on app quit and cold boot them on reopen;
make this visible in state/UI. Closing a tab continues to leave the machine
running. A failed graceful shutdown must not hold app quit indefinitely.

### Capacity

Replace the global macOS-specific assumption with host/backend-aware
admission limits and resource budgets. Account for memory and CPU across mixed
guest types. Preserve external-VM accounting where it is reliable; do not
offer capacity based on a count that omits competing VMs. The initial
Apple-host Linux limit should be established by measurement, not guessed from
the macOS two-VM limit.

### Compatibility

- Old machine folders and old macOS guest agents continue to work.
- Additive protocol fields have safe defaults; new operations fail with typed,
  actionable unsupported-capability errors when talking to an older host.
- Machine listings and JSON identify OS, architecture, image, and supported
  lifecycle/display capabilities without making existing fields ambiguous.
- Keep `--host`, remote claims, and remote watching semantics; version-check
  Linux operations and clearly report unsupported remote hosts.
- Rename “Virtual Macs” to “Virtual Machines” only as part of the user-facing
  integration, keeping existing settings/configuration keys readable.

## User experience and CLI

Provide an OS choice in the machine creation flow and show guest OS, image
version, provisioning/desktop readiness, CPU/memory, display size, and
lifecycle support in machine settings and listings.

Proposed command shape (final flags follow the existing CLI conventions):

```sh
sheprd machine create linux-dev --os linux --image ubuntu-wayland
sheprd machine open linux-dev
sheprd machine claim linux-dev
sheprd machine exec linux-dev -- uname -a
sheprd machine screenshot linux-dev /path/to/screen.png
sheprd machine release linux-dev
```

Preserve existing commands for macOS. Add CLI help, JSON output, tests,
documentation, and the managed agent skill in the same implementation change
that exposes corresponding actions. Respect confirmation for outward or
destructive actions and retain bounded request/response behavior.

## Phases and acceptance criteria

### 1. Feasibility spike — Linux on Apple silicon

Boot a minimal Linux guest through the Apple backend, attach the native view,
and prove a Wayland session appears in a Sheprd tab. Test keyboard, pointer,
screen capture, fixed-resolution coordinates, agent connection, actual 3D
renderer, and save/restore. Record supported OS versions and device
limitations.

**Gate:** do not commit to a product architecture or distribution image until
the screen and input path works reliably and lifecycle/resource limitations
are known. If Apple Virtualization cannot meet the desktop requirements,
evaluate a different backend without changing shared machine/CLI contracts.

### 2. Shared model and Apple backend boundary

Add typed guest/backend metadata and the narrow traits proven necessary by the
spike. Adapt the existing macOS VM implementation without changing its user
behavior. Read old machine folders and protocol payloads as before.

**Gate:** macOS machine start/open, settings, claims, image use, quit/save,
and remote forwarding still pass regression tests. No GPUI type leaks into
protocol/client layers, and Apple framework objects stay behind the backend
boundary.

### 3. Linux provisioning and lifecycle

Build the pinned ARM64 Wayland image workflow, machine creation, boot,
shutdown, readiness, identity regeneration, and capability-based lifecycle.

**Gate:** interrupted downloads/provisioning are recoverable; incomplete
machines are not listed as ready; clone identities are unique; close-tab,
quit, and reopen behavior matches reported lifecycle capabilities.

### 4. Linux agent and desktop interaction

Implement the Linux guest agent against the compatible bounded protocol and
Wayland session APIs. Add capability negotiation, screenshot validation,
input mapping, command execution, and restart/recovery behavior.

**Gate:** run a browser and native Wayland application through screenshots,
clicks, key combinations, and Unicode text; verify that input coordinates
match captured pixels and that unsupported permissions fail clearly.

### 5. Product and remote integration

Update Settings, tabs, CLI, status, capacity reporting, claims, images,
remote-host forwarding/watching, docs, and agent skill. Preserve existing
macOS names/configuration where compatibility requires it.

**Gate:** complete local and remote create/open/claim/exec/screenshot/input/
release flows; test mixed macOS and Linux machines and old remote hosts.

### 6. Hardening and future Windows implementation

Test failures, cancellation, host sleep, guest/session/agent restarts,
resource pressure, and image upgrades. Only when Windows is actively scoped,
add a Windows guest provisioner/agent and/or Windows-host backend based on
measured platform requirements.

**Gate:** each new Windows implementation satisfies the shared contracts and
capability tests; no Windows-specific branches are added to shared machine
management merely in anticipation.

## Likely code areas

| Concern | Current area to evolve |
| --- | --- |
| Machine domain and exports | `crates/sheprd-gpui/src/machine.rs` |
| VM lifecycle/backend and presentation wiring | `machine/vm.rs`, `browser/machine_tab.rs` |
| Guest-specific VM configuration and boot | `machine/configuration.rs` (split by responsibility as it grows) |
| Machine metadata, settings, images, and clone identity | `machine/store.rs`, `machine/settings.rs`, `machine/image.rs` |
| Host-side agent connection and protocol | `machine/agent.rs`, `machine/agent/wire.rs` |
| Existing macOS guest agent | `macos/sheprd-machine-agent/` |
| Linux guest agent and image build | New Linux-specific source/build packaging, location chosen with existing release packaging |
| Quit and capability-driven suspend | `machine/quit.rs`, `machine/vm.rs` |
| Protocol state, listings, capacity, capabilities | `crates/sheprd-control-protocol/src/protocol/tabs.rs` |
| Settings and CLI flows | `settings_window/`, `cli/`, `control/` in `crates/sheprd-gpui` |
| Remote watch and host forwarding | `machine/remote/`, `browser/remote_machine_tab.rs`, control tab handlers |

Keep Rust, shell, Python, and Swift source files under the repository's 1,000
line limit; split additions by responsibility.

## Verification plan

- Unit-test guest metadata defaults/migrations, capability selection,
  lifecycle transitions, and unsupported-operation errors.
- Keep golden/round-trip tests for the existing macOS agent protocol; test
  bounds on guest commands, screenshots, and remote frame payloads.
- Test image download checksum, cancellation/cleanup, atomic publication,
  and clone identity regeneration.
- Test Wayland capture/input with a real VM in an explicitly opt-in test
  using a selected VM image; never silently start a user's VM or daemon.
- Test coordinate scaling, display resize, key/modifier release, Unicode,
  desktop readiness, and agent reconnection end to end.
- Test macOS save/restore unchanged and Linux suspend behavior only when the
  backend reports it supported.
- Test mixed-guest capacity, close-tab/quit/reopen, old metadata, old agents,
  and remote-host version/capability failures.
- Run relevant quick checks under 10 seconds during implementation; leave
  longer VM and platform matrix checks for explicitly selected opt-in runs or
  CI according to their sandbox/cleanup requirements.

## Phase 1 spike findings

Measured on Apple M4 Max (16 cores, 64 GiB RAM), macOS 27.0.1 (build 26A434),
using a standalone Virtualization.framework + `VZVirtualMachineView` harness
booting pinned Ubuntu 24.04.5 LTS ARM64 (`noble-server-cloudimg-arm64.img`,
kernel `6.8.0-147-generic`, converted QCOW2 → 16 GiB sparse raw) with Sway 1.9,
XWayland 23.2.6, `grim`, `wtype`, `foot`, `epiphany-browser` (WebKitGTK 6.0),
and a prototype `AF_VSOCK` (port 7470, `VMADDR_CID_HOST` peer check) guest
agent speaking the existing 4-byte length-prefixed JSON wire protocol.

1. **Virtualization.framework API & macOS version floor:**
   - Required Linux EFI + graphics + input + vsock APIs (`VZEFIBootLoader`,
     `VZEFIVariableStore`, `VZGenericPlatformConfiguration`,
     `VZGenericMachineIdentifier`, `VZVirtioGraphicsDeviceConfiguration`,
     `VZVirtioGraphicsScanoutConfiguration`, `VZVirtioSocketDeviceConfiguration`,
     `VZUSBKeyboardConfiguration`,
     `VZUSBScreenCoordinatePointingDeviceConfiguration`) are available from
     macOS 13.0 (`VZVirtioGraphicsScanout` runtime display and
     `automaticallyReconfiguresDisplay` from macOS 14.0, matching Sheprd's
     existing macOS VM baseline). All bindings already exist in the workspace's
     `objc2-virtualization 0.3.2`.
   - Boot to Wayland desktop + guest agent `hello` took ~8 seconds on a
     provisioned raw disk.
2. **Compositor, capture, input, and coordinate accuracy:**
   - Sway (wlroots) launched via `getty@tty1` autologin with
     `WLR_NO_HARDWARE_CURSORS=1` provides a clean Wayland session
     (`WAYLAND_DISPLAY=wayland-1`, `DISPLAY=:0` via XWayland, `SWAYSOCK` set)
     and starts the guest agent inside the graphical session.
   - `grim` (`wlr-screencopy-unstable-v1`) captures unattended PNG screenshots
     with zero permission prompts.
   - `swaymsg seat - cursor set/press/release` delivered exact `(640.0, 400.0)`
     clicks to a fullscreen GTK3 Wayland probe at `1280x800` (0.0 px error).
   - `wtype` (`zwp_virtual_keyboard_unstable_v1`) delivered ASCII, accented
     UTF-8, CJK, and emoji (`Hello café 日本語 🚀`) plus `ctrl+a`
     (`GDK_CONTROL_MASK`) cleanly. Images should include `fonts-noto-cjk` and
     `fonts-noto-color-emoji` so non-Latin/emoji glyphs render instead of hex
     boxes, and pre-disable Epiphany's first-run default-browser prompt
     (`org.gnome.Epiphany ask-for-default false`).
   - Post-boot display resize worked via
     `VZVirtioGraphicsScanout.reconfigure(sizeInPixels:)` followed by
     `swaymsg output Virtual-1 mode 1440x900` (verified `1440x900` `grim`
     capture). Enabling `automaticallyReconfiguresDisplay = true` at initial
     boot caused a vCPU spin before `virtio-gpu` settled, confirming the plan
     to ship fixed resolution first and add explicit resize later.
3. **3D renderer and browser usability:**
   - `VZVirtioGraphicsDeviceConfiguration` exposes 2D Virtio-GPU only; both
     EGL (GBM/Wayland) and GLX (XWayland) use software rendering:
     `Mesa llvmpipe (LLVM 20.1.2, 128 bits)`, `Accelerated: no`, OpenGL 4.5 /
     GLES 3.2.
   - Native Wayland apps (`foot`, GTK3) and WebKitGTK (`epiphany-browser`)
     rendered crisply and responsively on `llvmpipe`.
4. **Save/restore vs. graceful shutdown:**
   - Although `validateSaveRestoreSupport()` returned supported on macOS 27.0.1
     and `saveMachineStateTo(url:)` wrote a 236 MiB state file,
     `restoreMachineStateFrom(url:)` failed in a fresh process (both with and
     without a serial port) with `VZErrorDomain Code=12 ("permission denied")`.
   - ACPI graceful shutdown (`requestStop()`) stopped the full Wayland guest
     cleanly in `0.51–0.76s` (`guestDidStopVirtualMachine` fired reliably).
   - **Decision:** report `supports_save_restore = false` for Linux guests;
     gracefully shut down on app quit (bounded timeout, then force stop) and
     cold boot (~8s) on reopen.
5. **Capacity, resource footprint, and provisioning caveats:**
   - `4/4` simultaneous Linux VMs started concurrently on Apple silicon,
     confirming Apple's 2-VM cap applies only to `VZMacPlatformConfiguration`
     (macOS guests), not `VZGenericPlatformConfiguration`. Shared capacity
     accounting (`machine/lend.rs` currently counts all
     `com.apple.Virtualization.VirtualMachine` processes against
     `MAX_ACTIVE_MACHINES = 2`) must distinguish macOS-slot limits from
     CPU/memory budgets for mixed guests.
   - Idle 4 vCPU / 4 GiB Linux Wayland guest used ~750–985 MiB host RSS and
     ~0.1% CPU.
   - **Image/boot hardening required:** configure `GRUB_TIMEOUT=0`,
     `GRUB_RECORDFAIL_TIMEOUT=0`, and `fsck.repair=yes` so unclean stops never
     stall at GRUB/initramfs; do not set `console=hvc0` unless a virtio serial
     port is attached (otherwise systemd stalls ~90s waiting for `dev-hvc0`);
     pre-build or reboot between `growpart`/`resize2fs` and heavy package
     installs (Ubuntu 24.04 kernel 6.8 hit transient in-memory ELF/page faults
     during first-boot online `resize2fs` + heavy `dpkg` unpack, with zero
     errors after reboot; Ubuntu 26.04.1 LTS `resolute` is also available to
     evaluate for the production image).

## Implementation progress

- **Phase 1 (Feasibility spike):** Complete. Findings recorded above.
- **Phase 2 (Shared model and Apple backend boundary):** Complete. Added
  protocol `GuestOs`, `GuestArch`, `MachineCapabilities`, `AgentCapabilities`,
  versioned `guest.json` metadata with legacy macOS fallback, isolated
  `objc2-virtualization` behind `machine/backend/apple.rs`, and implemented
  capability-driven quit/suspend handling.
- **Phase 3 (Linux provisioning and lifecycle):** Complete. Added pinned ARM64
  Ubuntu 24.04.5 LTS Wayland catalog with SHA-256 verification and corrupt-file
  cleanup, pure-Rust bounded QCOW2-to-sparse-raw converter (validated against
  the real Ubuntu 24.04.5 ARM64 cloud image), pure-Rust NoCloud `cidata` ISO
  9660/Joliet generator for initial provisioning and per-clone identity
  regeneration (`/etc/machine-id`, SSH host keys, hostname), EFI variable store
  creation and read-only `seed.iso` attachment, atomic `.install-*` staging and
  publication, and Linux desktop readiness tracking (`machine/readiness.rs`).
- **Phase 4 (Linux guest agent and desktop interaction):** Complete. Implemented
  the Linux Wayland guest agent (`linux/sheprd-machine-agent/{agent.py,wayland.py}`)
  speaking the framed bounded vsock protocol on port 7470 with `VMADDR_CID_HOST`
  peer enforcement, dynamic Wayland/Sway session recovery, capability
  negotiation, `grim` PNG capture with header/dimension validation, Sway IPC
  pointer clicks with scale conversion and bounds checks, `wtype` Unicode text
  and portable key/modifier release, process-group `exec` with timeout/tail
  truncation, systemd user session embedding in `seed.rs`, and host-side
  `Refusal::{NotReady, Unsupported}` + PNG dimension bounds handling.
- **Next:** Phase 5 (Product and remote integration).

## Open questions remaining

- **Windows target priority (formerly Q6):** Which future Windows target is
  desired first — a Windows guest on a Mac, a Windows host backend, or both?
  Their required backends and drivers differ.
- **Compositor choice for UX:** Sway proved reliable for unattended capture
  and input; decide whether to ship Sway (tiling, minimal) or evaluate
  `labwc` (stacking, wlroots-based, also in Ubuntu 24.04 `universe`) in Phase 3
  for a more conventional desktop windowing feel.
