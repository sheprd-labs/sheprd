//! `sheprd machine lend`, which another computer's Sheprd runs here over
//! SSH to borrow a virtual Mac: one request as JSON on standard input, sent
//! to this app as relayed by that computer ([`Via::Lend`]), and one answer
//! as JSON on standard output. The app decides what it serves; this only
//! carries it, and composes pictures, which are too large for the control
//! socket, from a screenshot in a private folder.
//!
//! Failures of the app's own come back inside the answer, so the borrower
//! reads their code and message; a missing app fails as any command does,
//! with its exit code.

use super::{Context, Output, rejected};
use crate::control::{
    ErrorCode,
    client::send,
    protocol::{
        Call, Caller, Request, Response, Via,
        lend::{
            LendAnswer, LendCall, LendRequest, MAX_FRAME_WIDTH, MAX_LEND_CALL, MIN_FRAME_WIDTH,
        },
        tabs::{Reply, TabOp},
    },
};
use base64::Engine as _;
use std::io::Read as _;

/// The largest screenshot read back: a large display's PNG.
const MAX_PICTURE: u64 = 64 << 20;

pub(super) fn serve(_: &Context) -> crate::Result<Output> {
    let mut text = Vec::new();
    std::io::stdin()
        .take(MAX_LEND_CALL as u64 + 1)
        .read_to_end(&mut text)?;
    if text.len() > MAX_LEND_CALL {
        return Err(rejected(
            ErrorCode::InvalidRequest,
            "The request is too large",
        ));
    }
    let call: LendCall = serde_json::from_slice(&text)?;
    let answer = answer(call)?;
    let bytes = serde_json::to_vec(&answer)?;
    Output::bytes(bytes, &())
}

fn answer(call: LendCall) -> crate::Result<LendAnswer> {
    let via = Via::Lend {
        borrower: call.borrower,
        owner: call.owner,
    };
    if !via.is_valid() {
        return Ok(refusal("The borrower's names must be printable and short"));
    }
    // Only who relayed it: no pane or window of this computer is the
    // caller's, whatever the SSH session's variables say.
    let caller = Caller {
        via: Some(via),
        ..Caller::default()
    };
    Ok(match call.request {
        // Pictures cross only as `Picture` and `Frame`, written where this
        // command chooses: never at a path the borrower names.
        LendRequest::Tab {
            op: TabOp::MachineScreenshot { .. } | TabOp::MachinePeek { .. },
        } => refusal("Ask for a picture or a frame instead"),
        LendRequest::Tab { op } if op.lendable() => LendAnswer::Response {
            response: send(&Request::Tab(Call { caller, op }))?,
        },
        LendRequest::Tab { .. } => refusal("Another computer may only ask for virtual Mac work"),
        LendRequest::Picture { name } => match screenshot(caller, name, false)? {
            Ok(png) => picture(&png)?,
            Err(response) => LendAnswer::Response { response },
        },
        LendRequest::Frame { name, width, since } => {
            if !(MIN_FRAME_WIDTH..=MAX_FRAME_WIDTH).contains(&width) {
                return Ok(refusal("A frame is 160 to 3840 pixels wide"));
            }
            match screenshot(caller, name, true)? {
                Ok(png) => {
                    let frame = crate::machine::remote::frame::make(&png, width, since.as_deref())
                        .map_err(crate::machine::Error::from)?;
                    LendAnswer::Frame {
                        hash: frame.hash,
                        width: frame.width,
                        height: frame.height,
                        jpeg: frame
                            .jpeg
                            .map(|jpeg| base64::engine::general_purpose::STANDARD.encode(jpeg)),
                    }
                }
                Err(response) => LendAnswer::Response { response },
            }
        }
    })
}

fn refusal(message: &str) -> LendAnswer {
    LendAnswer::Response {
        response: Response::error(ErrorCode::InvalidRequest, message),
    }
}

/// The machine's screen as a PNG, or the app's refusal. The app writes it
/// into a folder only this user reads, removed when this returns. A
/// watcher's frame (`watching`) needs no claim; an agent's screenshot does.
fn screenshot(
    caller: Caller,
    name: String,
    watching: bool,
) -> crate::Result<Result<Vec<u8>, Response>> {
    let folder = tempfile::Builder::new().prefix("sheprd-lend-").tempdir()?;
    let path = folder.path().join("screen.png");
    let Some(text) = path.to_str() else {
        return Err(crate::Error::InvalidLocalPage);
    };
    let path_text = text.to_owned();
    let op = if watching {
        TabOp::MachinePeek {
            name,
            path: path_text,
        }
    } else {
        TabOp::MachineScreenshot {
            name,
            path: path_text,
        }
    };
    match send(&Request::Tab(Call { caller, op }))? {
        Response::Tab {
            reply: Reply::MachineScreenshot { .. },
        } => {}
        response => return Ok(Err(response)),
    }
    let mut png = Vec::new();
    std::fs::File::open(&path)?
        .take(MAX_PICTURE + 1)
        .read_to_end(&mut png)?;
    if png.len() as u64 > MAX_PICTURE {
        return Ok(Err(Response::error(
            ErrorCode::Failed,
            "The screenshot is larger than a picture can be",
        )));
    }
    Ok(Ok(png))
}

fn picture(png: &[u8]) -> crate::Result<LendAnswer> {
    let (width, height) =
        image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(|error| {
                crate::machine::Error::from(crate::machine::remote::Error::Decode(error))
            })?;
    Ok(LendAnswer::Picture {
        png: base64::engine::general_purpose::STANDARD.encode(png),
        width,
        height,
    })
}

#[cfg(test)]
mod tests;
