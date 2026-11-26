//! Host VM backend boundary.
//!
//! Keeps platform-specific hypervisor types, unsafe framework bindings, and
//! thread-affinity proofs inside the host backend implementation rather than
//! leaking them into shared machine management, storage, or protocol layers.

#[cfg(target_os = "macos")]
pub(super) mod apple;

/// Why a running VM stopped without a host-initiated force stop.
#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StopReason {
    GuestStopped,
    Failed(String),
}
