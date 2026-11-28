//! The attach wire between the `herdr-acp` chat program and its clients.
#![doc = include_str!("../README.md")]

mod agents;
pub mod assist;
pub mod brief;
mod change;
pub mod display;
mod endpoint;
mod error;
pub mod excerpt;
mod frame;
mod guest;
pub mod handoff;
mod harnesses;
mod media;
mod model;
pub mod mux;
pub mod naming;
mod origin;
mod private;
pub mod restore;
mod settings;
pub mod sharing;
pub mod thread;
mod wire;

pub use agents::{Agent, UnknownAgent};
pub use brief::{Brief, MAX_BRIEFS};
pub use change::Change;
pub use endpoint::{
    AGENT_CHAT_ENV, ENDPOINT_ENV, EndpointId, TOKEN, endpoint_address, endpoint_dir,
    endpoint_dir_with, endpoint_path,
};
pub use error::{Error, Result};
pub use frame::{MAX_FRAME_SIZE, encode_frame, read_frame, write_frame};
pub use guest::{
    GUEST_ALLOW, GUEST_DENY, Guest, GuestPrompt, MAX_GUEST_NAME, MAX_GUEST_PROMPT_SIZE,
    MAX_GUEST_TOOL, MAX_GUESTS_WAITING, shows_as_written,
};
pub use harnesses::Harnesses;
pub use media::{Author, Image, ImageType, MAX_CAPTION, MAX_IMAGE_DATA};
pub use model::*;
pub use origin::{Device, Origin, Via};
pub use settings::{MAX_SETTINGS, SETTINGS_ENV, Setting, Settings};
pub use wire::*;

#[cfg(test)]
mod tests;
