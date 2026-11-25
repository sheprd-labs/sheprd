//! Watching a virtual Mac on another computer: its picture cannot leave the
//! Sheprd running it, so a tab here draws frames that Sheprd makes there
//! ([`super::frame`]) and sends clicks and keys back through Sheprd Agent,
//! as `machine click`, `type`, and `key` do.
//!
//! One frame is in flight at a time, so a slow link lowers the frame rate
//! instead of queuing, and the newest frame is the one drawn. Frames shrink
//! while they take long and grow back while they are quick. Nothing is
//! fetched while the tab is not drawn.

use super::{Error, link};
use crate::{
    control::protocol::{
        ErrorCode, Response,
        lend::{LendAnswer, LendCall, LendRequest, MAX_FRAME_WIDTH, MIN_FRAME_WIDTH},
        tabs::{MAX_MACHINE_TYPE_CHARS, TabOp},
    },
    teleport::Host,
};
use base64::Engine as _;
use gpui::{FocusHandle, Image, ImageFormat};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// The width a watcher starts at.
pub(crate) const START_WIDTH: u32 = 1280;
/// A frame slower than this halves the next one's width.
const SLOW: Duration = Duration::from_millis(500);
/// A frame quicker than this lets the width grow again.
const QUICK: Duration = Duration::from_millis(150);
/// The pause between frames, so a quiet screen costs little.
pub(crate) const GAP: Duration = Duration::from_millis(150);
/// How long after the tab was last drawn frames keep coming.
pub(crate) const SHOWN: Duration = Duration::from_secs(2);
/// What input from a watcher waits for.
const INPUT_WAIT: Duration = Duration::from_secs(60);
/// Who the lending computer says holds the screen when a person here
/// clicks or types.
const OWNER: &str = "a person watching";

/// What a watcher holds for its tab.
pub(crate) struct Watch {
    pub(crate) frame: Option<Arc<Image>>,
    /// The machine's screen in pixels, which clicks are in.
    pub(crate) screen: (u32, u32),
    hash: Option<String>,
    pub(crate) width: u32,
    /// What went wrong last, shown over the frame; cleared by a good frame.
    pub(crate) problem: Option<String>,
    /// When the tab was last drawn: frames come only while it shows.
    pub(crate) drawn: Instant,
    /// How long the last frame took, for the tab to say when it is slow.
    pub(crate) took: Option<Duration>,
    pub(crate) focus: FocusHandle,
    pub(crate) running: bool,
}

impl Watch {
    pub(crate) fn new(focus: FocusHandle) -> Self {
        Self {
            frame: None,
            screen: (0, 0),
            hash: None,
            width: START_WIDTH,
            problem: None,
            drawn: Instant::now(),
            took: None,
            focus,
            running: false,
        }
    }

    /// The request for the next frame.
    pub(crate) fn next(&self, name: &str) -> LendRequest {
        LendRequest::Frame {
            name: name.to_owned(),
            width: self.width,
            since: self.hash.clone(),
        }
    }

    /// Takes what the lending computer answered after `took`.
    pub(crate) fn update(&mut self, answer: Result<Fetched, String>, took: Duration) {
        self.took = Some(took);
        self.width = next_width(self.width, took);
        match answer {
            Ok(fetched) => {
                self.problem = None;
                self.hash = Some(fetched.hash);
                self.screen = fetched.screen;
                if let Some(frame) = fetched.frame {
                    self.frame = Some(frame);
                }
            }
            Err(problem) => self.problem = Some(problem),
        }
    }
}

/// The most input waiting to be sent; beyond it, keys are dropped and the
/// tab says so, as a link that slow cannot keep up with typing anyway.
const MAX_QUEUED: usize = 256;

/// Clicks and keys waiting for the one before them: each is its own run on
/// the other computer, and runs in parallel would arrive in any order.
#[derive(Debug, Default)]
pub(crate) struct Inputs {
    queue: std::collections::VecDeque<TabOp>,
    sending: bool,
}

impl Inputs {
    /// Queues `op`. `Ok(true)` when nothing is being sent, so the caller
    /// starts sending; `Err` when the queue is full and `op` was dropped.
    pub(crate) fn push(&mut self, op: TabOp) -> Result<bool, ()> {
        if self.queue.len() >= MAX_QUEUED {
            return Err(());
        }
        self.queue.push_back(op);
        Ok(!std::mem::replace(&mut self.sending, true))
    }

    /// The next input to send, with the text typed after it in one, so a
    /// slow link types a word at once; `None` when all is sent, which ends
    /// the sending.
    pub(crate) fn next(&mut self) -> Option<TabOp> {
        let Some(mut op) = self.queue.pop_front() else {
            self.sending = false;
            return None;
        };
        if let TabOp::MachineType { text, .. } = &mut op {
            while let Some(TabOp::MachineType { text: more, .. }) = self.queue.front() {
                if text.chars().count() + more.chars().count() > MAX_MACHINE_TYPE_CHARS {
                    break;
                }
                text.push_str(more);
                self.queue.pop_front();
            }
        }
        Some(op)
    }
}

/// A frame read and checked.
pub(crate) struct Fetched {
    hash: String,
    screen: (u32, u32),
    frame: Option<Arc<Image>>,
}

/// The width of the next frame after one that took `took`.
pub(crate) fn next_width(width: u32, took: Duration) -> u32 {
    let width = if took > SLOW {
        width / 2
    } else if took < QUICK {
        width.saturating_add(width / 4)
    } else {
        width
    };
    width.clamp(MIN_FRAME_WIDTH, MAX_FRAME_WIDTH)
}

/// Asks `host` for a frame of machine `name`. It blocks: run it on a worker.
pub(crate) fn fetch(
    host: &Host,
    label: &str,
    request: LendRequest,
    cancel: &AtomicBool,
) -> Result<Fetched, String> {
    let call = LendCall {
        borrower: link::BORROWER.clone(),
        owner: OWNER.to_owned(),
        request,
    };
    let wait = Duration::from_secs(60);
    match link::run(host, label, &call, wait, cancel) {
        Ok(LendAnswer::Frame {
            hash,
            width,
            height,
            jpeg,
        }) => {
            let frame = jpeg
                .map(|jpeg| decode(&jpeg))
                .transpose()
                .map_err(|error| crate::i18n::Localize::localize(&error))?;
            Ok(Fetched {
                hash,
                screen: (width, height),
                frame,
            })
        }
        Ok(LendAnswer::Response {
            response: Response::Error { message, .. },
        }) => Err(message),
        Ok(_) => Err(format!("{label} answered with something else")), // i18n: data
        Err(error) => Err(crate::i18n::Localize::localize(&error)),
    }
}

/// A JPEG from the other computer, checked before it is drawn.
fn decode(jpeg: &str) -> Result<Arc<Image>, Error> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(jpeg)
        .map_err(Error::Base64)?;
    image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Jpeg)
        .into_dimensions()
        .map_err(Error::Decode)?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Jpeg, bytes)))
}

/// Sends a click, text, or a key from the person watching; the answer's
/// message when the machine refused it, as when an agent holds it.
pub(crate) fn input(host: &Host, label: &str, op: TabOp, cancel: &AtomicBool) -> Option<String> {
    let call = LendCall {
        borrower: link::BORROWER.clone(),
        owner: OWNER.to_owned(),
        request: LendRequest::Tab { op },
    };
    match link::run(host, label, &call, INPUT_WAIT, cancel) {
        Ok(LendAnswer::Response {
            response: Response::Error { code, message },
        }) => Some(match code {
            ErrorCode::Busy => format!("Read only: {message}"), // i18n: data
            _ => message,
        }),
        Ok(_) => None,
        Err(error) => Some(crate::i18n::Localize::localize(&error)),
    }
}

/// A key as `machine key` names it, from GPUI's name; `None` for keys a
/// Mac has no name for here.
pub(crate) fn key_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "enter" => "return",
        "escape" => "escape",
        "tab" => "tab",
        "backspace" => "delete",
        "delete" => "forwarddelete",
        "space" => "space",
        "left" => "left",
        "right" => "right",
        "up" => "up",
        "down" => "down",
        "home" => "home",
        "end" => "end",
        "pageup" => "pageup",
        "pagedown" => "pagedown",
        _ => return None,
    })
}

/// The point of the machine's `screen` under `point`, given where the frame
/// is drawn, at the frame's aspect inside `area`; `None` outside it.
pub(crate) fn screen_point(
    point: (f32, f32),
    area: (f32, f32, f32, f32),
    screen: (u32, u32),
) -> Option<(u32, u32)> {
    let (x, y, width, height) = area;
    let (screen_width, screen_height) = (screen.0 as f32, screen.1 as f32);
    if screen_width <= 0. || screen_height <= 0. || width <= 0. || height <= 0. {
        return None;
    }
    let scale = (width / screen_width).min(height / screen_height);
    let (drawn_width, drawn_height) = (screen_width * scale, screen_height * scale);
    let left = x + (width - drawn_width) / 2.;
    let top = y + (height - drawn_height) / 2.;
    let (dx, dy) = (point.0 - left, point.1 - top);
    if dx < 0. || dy < 0. || dx >= drawn_width || dy >= drawn_height {
        return None;
    }
    Some(((dx / scale) as u32, (dy / scale) as u32))
}

#[cfg(test)]
mod tests;
