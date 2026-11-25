#![allow(clippy::unwrap_used)]
use super::*;
use image::{ImageBuffer, Rgb};

fn png(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let screen: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(width, height, |x, _| Rgb([shade, (x % 256) as u8, 0]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    screen.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn scales_down_keeping_the_screen_size_for_clicks() {
    let frame = make(&png(400, 200, 10), 200, None).unwrap();
    assert_eq!((frame.width, frame.height), (400, 200));
    let jpeg = image::load_from_memory(&frame.jpeg.unwrap()).unwrap();
    assert_eq!((jpeg.width(), jpeg.height()), (200, 100));
}

#[test]
fn never_scales_up() {
    let frame = make(&png(100, 50, 10), 800, None).unwrap();
    let jpeg = image::load_from_memory(&frame.jpeg.unwrap()).unwrap();
    assert_eq!(jpeg.width(), 100);
}

#[test]
fn an_unchanged_screen_sends_nothing_and_a_changed_one_a_new_hash() {
    let first = make(&png(64, 64, 1), 64, None).unwrap();
    let same = make(&png(64, 64, 1), 64, Some(&first.hash)).unwrap();
    assert!(same.jpeg.is_none());
    assert_eq!(same.hash, first.hash);
    let changed = make(&png(64, 64, 2), 64, Some(&first.hash)).unwrap();
    assert!(changed.jpeg.is_some());
    assert_ne!(changed.hash, first.hash);
}

#[test]
fn a_picture_that_is_not_a_png_is_refused() {
    assert!(matches!(
        make(b"not a png", 64, None),
        Err(Error::Decode(_))
    ));
}

#[test]
fn the_same_screen_at_another_width_is_a_new_frame() {
    let small = make(&png(400, 200, 3), 200, None).unwrap();
    let wide = make(&png(400, 200, 3), 400, Some(&small.hash)).unwrap();
    assert!(wide.jpeg.is_some());
    assert_ne!(wide.hash, small.hash);
}
