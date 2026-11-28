//! The browser adapter runs on a dedicated worker. No Tokio or host services.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod browser;
