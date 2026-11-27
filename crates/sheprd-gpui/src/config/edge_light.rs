//! Appearance of the native perimeter trail for the focused agent.
use crate::i18n::fl;
use serde::{Deserialize, Deserializer, de::Error as _};
use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct EdgeLight {
    #[serde(deserialize_with = "color")]
    pub color: Option<u32>,
    #[serde(deserialize_with = "width")]
    pub width: u8,
    #[serde(deserialize_with = "trail")]
    pub trail: u8,
}

impl EdgeLight {
    pub(crate) const WIDTH: RangeInclusive<u8> = 1..=12;
    pub(crate) const TRAIL: RangeInclusive<u8> = 5..=50;
}

impl Default for EdgeLight {
    fn default() -> Self {
        Self {
            color: None,
            width: 5,
            trail: 16,
        }
    }
}

fn color<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    let text = String::deserialize(d)?;
    if text.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let digits = text
        .strip_prefix('#')
        .filter(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    digits
        .and_then(|s| u32::from_str_radix(s, 16).ok())
        .map(Some)
        .ok_or_else(|| D::Error::custom(fl!("error-config-invalid-edge-light-color")))
}

fn width<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let value = u8::deserialize(d)?;
    if EdgeLight::WIDTH.contains(&value) {
        Ok(value)
    } else {
        Err(D::Error::custom(fl!(
            "error-config-invalid-edge-light-width"
        )))
    }
}

fn trail<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let value = u8::deserialize(d)?;
    if EdgeLight::TRAIL.contains(&value) {
        Ok(value)
    } else {
        Err(D::Error::custom(fl!(
            "error-config-invalid-edge-light-trail"
        )))
    }
}
