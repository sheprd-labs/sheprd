//! `sheprd machine lend`: what another computer's Sheprd sends over SSH to
//! borrow a virtual Mac here, and what comes back. One [`LendCall`] as JSON
//! on standard input, one [`LendAnswer`] as JSON on standard output.
//!
//! Pictures cross here rather than through the app's control socket, whose
//! messages stay small ([`super::MAX_MESSAGE`]): the command asks the app
//! for a screenshot in a private file and reads it itself.

use super::{Response, tabs::TabOp};
use serde::{Deserialize, Serialize};

/// The largest call: a request, as the control socket bounds one.
pub const MAX_LEND_CALL: usize = super::MAX_MESSAGE;
/// The largest answer: a screenshot of a large display, in base64.
pub const MAX_LEND_ANSWER: usize = 96 << 20;
/// The narrowest and widest frame a watcher asks for, in pixels.
pub const MIN_FRAME_WIDTH: u32 = 160;
pub const MAX_FRAME_WIDTH: u32 = 3840;

/// A request from computer `borrower`, on behalf of `owner` there: a pane,
/// a terminal, or the person watching a machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LendCall {
    pub borrower: String,
    pub owner: String,
    pub request: LendRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LendRequest {
    /// A virtual Mac request the app answers; only [`TabOp::lendable`] ones.
    Tab { op: TabOp },
    /// The machine's screen as a PNG.
    Picture { name: String },
    /// The machine's screen scaled to `width`, as a JPEG, unless it is the
    /// same as the frame whose hash is `since`.
    Frame {
        name: String,
        width: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LendAnswer {
    /// The app's answer, also every failure.
    Response { response: Response },
    /// A PNG in base64, `width` by `height` pixels.
    Picture {
        png: String,
        width: u32,
        height: u32,
    },
    /// A frame: `jpeg` in base64, absent when the screen still matches
    /// `since`. `hash` names this screen for the next request; `width` and
    /// `height` are the screen's own pixels, which clicks use.
    Frame {
        hash: String,
        width: u32,
        height: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        jpeg: Option<String>,
    },
}

#[cfg(test)]
mod tests;
