//! A watcher's frame: the machine's screen, scaled down and compressed, or
//! nothing when it has not changed. Made on the lending computer from the
//! PNG Sheprd Agent takes, so the agent inside every machine stays as it is.

use super::Error;
use base64::Engine as _;
use image::{ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType};
use sha2::{Digest as _, Sha256};

/// Good enough to read text on a scaled screen, a fraction of a PNG's size.
const QUALITY: u8 = 70;

/// A frame as the watcher receives it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    /// Names this screen, for the next request's `since`.
    pub(crate) hash: String,
    /// The screen's own pixels, which clicks use.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The JPEG, absent when the screen still matches `since`.
    pub(crate) jpeg: Option<Vec<u8>>,
}

/// The frame of the screen in `png`, at most `width` pixels wide.
pub(crate) fn make(png: &[u8], width: u32, since: Option<&str>) -> Result<Frame, Error> {
    let screen = image::load_from_memory_with_format(png, ImageFormat::Png)
        .map_err(Error::Decode)?
        .into_rgb8();
    let (full_width, full_height) = screen.dimensions();
    let size = scaled_size((full_width, full_height), width);
    // The size drawn is part of what the hash names: a frame asked for at
    // another width is new even when the screen is not.
    let mut digest = Sha256::new();
    digest.update(size.0.to_be_bytes());
    digest.update(size.1.to_be_bytes());
    digest.update(screen.as_raw());
    let hash = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest.finalize()[..16]);
    if since == Some(hash.as_str()) {
        return Ok(Frame {
            hash,
            width: full_width,
            height: full_height,
            jpeg: None,
        });
    }
    let scaled = if size == (full_width, full_height) {
        screen
    } else {
        image::imageops::resize(&screen, size.0, size.1, FilterType::Triangle)
    };
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, QUALITY)
        .encode_image(&scaled)
        .map_err(Error::Encode)?;
    Ok(Frame {
        hash,
        width: full_width,
        height: full_height,
        jpeg: Some(jpeg),
    })
}

/// The size of a frame at most `width` wide of a screen of `full` pixels:
/// never larger than the screen.
fn scaled_size(full: (u32, u32), width: u32) -> (u32, u32) {
    let (full_width, full_height) = full;
    if width >= full_width || full_width == 0 {
        return full;
    }
    let height = (u64::from(full_height) * u64::from(width) / u64::from(full_width)).max(1);
    (width, u32::try_from(height).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests;
