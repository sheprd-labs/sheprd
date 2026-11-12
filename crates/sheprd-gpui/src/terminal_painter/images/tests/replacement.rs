use super::*;
use sheprd_client::{
    ImageStore,
    protocol::{SurfaceGraphicsAsset, SurfaceGraphicsScene},
};

fn replacement(store: &mut ImageStore, fingerprint: u64) -> SurfaceImage {
    let mut key = key(SurfaceGraphicsFormat::Rgba, 1, 1);
    key.data_fingerprint = fingerprint;
    let mut scene = SurfaceGraphicsScene {
        assets: vec![SurfaceGraphicsAsset {
            key: key.clone(),
            data: vec![7; 4],
        }],
        placements: vec![placement(key.clone())],
        retained_assets: vec![],
    };
    store.receive_shown_scene(&mut scene);
    store.published().get(&key).unwrap().clone()
}

fn paint(cache: &mut ImageCache, image: &SurfaceImage, now: Instant) -> Option<Arc<RenderImage>> {
    let lookup = cache.lookup(image.serial(), now);
    cache.for_paint(image, lookup, now)
}

#[test]
fn failed_decode_clears_the_fallback_for_subsequent_replacements() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let a = replacement(&mut store, 1);
    ready(&mut cache, a.serial(), 1, now);
    let old = paint(&mut cache, &a, now).unwrap();
    let b = replacement(&mut store, 2);
    assert!(Arc::ptr_eq(&old, &paint(&mut cache, &b, now).unwrap()));
    cache.finish(b.serial(), Err(Error::PaneImageLimit));
    assert!(paint(&mut cache, &b, now).is_none());
    assert!(cache.shown.is_empty());
    let c = replacement(&mut store, 3);
    assert!(paint(&mut cache, &c, now).is_none());
}

#[test]
fn evicted_textures_cannot_be_used_as_fallbacks() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let a = replacement(&mut store, 1);
    ready(&mut cache, a.serial(), 1, now);
    assert!(paint(&mut cache, &a, now).is_some());
    assert_eq!(cache.begin(now + IDLE).len(), 1);
    assert!(cache.shown.is_empty());
    let b = replacement(&mut store, 2);
    assert!(paint(&mut cache, &b, now + IDLE).is_none());
}

#[test]
fn fallback_is_used_this_paint_and_late_completion_takes_over() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let a = replacement(&mut store, 1);
    ready(&mut cache, a.serial(), 1, now);
    let old = paint(&mut cache, &a, now).unwrap();
    let b = replacement(&mut store, 2);
    assert!(Arc::ptr_eq(&old, &paint(&mut cache, &b, now).unwrap()));
    let c = replacement(&mut store, 3);
    // The scene moved on to C before B finished: B still becomes the newest
    // ready frame, or a stream faster than the decoder would freeze on A.
    let finished = Arc::new(texture(RgbaImage::new(1, 1)));
    cache.finish(b.serial(), Ok(finished.clone()));
    let later = now + IDLE / 2;
    cache.begin(later);
    assert!(Arc::ptr_eq(
        &finished,
        &paint(&mut cache, &c, later).unwrap()
    ));
}

#[test]
fn a_slow_older_decode_never_moves_the_fallback_back() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let a = replacement(&mut store, 1);
    ready(&mut cache, a.serial(), 1, now);
    assert!(paint(&mut cache, &a, now).is_some());
    let (older, newer) = (replacement(&mut store, 2), replacement(&mut store, 3));
    assert!(paint(&mut cache, &older, now).is_some());
    assert!(paint(&mut cache, &newer, now).is_some());
    assert!(cache.pending.len() <= MAX_DECODES);
    let newest = Arc::new(texture(RgbaImage::new(1, 1)));
    cache.finish(newer.serial(), Ok(newest.clone()));
    cache.finish(older.serial(), Ok(Arc::new(texture(RgbaImage::new(1, 1)))));
    assert!(cache.pending.is_empty());
    let next = replacement(&mut store, 4);
    assert!(Arc::ptr_eq(
        &newest,
        &paint(&mut cache, &next, now).unwrap()
    ));
}

#[test]
fn shown_mappings_are_bounded_by_cache_entries() {
    let mut cache = ImageCache::default();
    let start = Instant::now();
    for index in 0..MAX_ENTRIES * 2 {
        let now = start + IDLE * u32::try_from(index).unwrap();
        cache.begin(now);
        let mut store = ImageStore::default();
        let image = replacement(&mut store, 1);
        ready(&mut cache, image.serial(), 1, now);
        assert!(paint(&mut cache, &image, now).is_some());
        assert!(cache.shown.len() <= cache.entries.len());
        assert!(cache.shown.len() <= MAX_ENTRIES);
    }
}

#[test]
fn a_stream_of_replacements_keeps_the_cache_small_and_moving() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let first = replacement(&mut store, 1);
    ready(&mut cache, first.serial(), 1, now);
    assert!(paint(&mut cache, &first, now).is_some());
    let mut last = None;
    for fingerprint in 2..400 {
        let frame = replacement(&mut store, fingerprint);
        cache.begin(now);
        let shown = paint(&mut cache, &frame, now).unwrap();
        // The decode of this frame finishes before the next one arrives.
        let fresh = Arc::new(texture(RgbaImage::new(1, 1)));
        cache.finish(frame.serial(), Ok(fresh.clone()));
        cache.begin(now);
        // While this frame decodes, the previous finished frame is shown.
        if let Some(last) = &last {
            assert!(Arc::ptr_eq(&shown, last));
        }
        assert!(
            cache.entries.len() <= MAX_DECODES + 2,
            "superseded frames must not pile up: {} entries",
            cache.entries.len()
        );
        last = Some(fresh);
    }
    // The newest finished frame is the one the next paint falls back to.
    let next = replacement(&mut store, 400);
    assert!(Arc::ptr_eq(
        &last.unwrap(),
        &paint(&mut cache, &next, now).unwrap()
    ));
}

#[test]
fn an_older_frame_placed_again_is_shown_and_not_decoded_forever() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let a = replacement(&mut store, 1);
    ready(&mut cache, a.serial(), 1, now);
    assert!(paint(&mut cache, &a, now).is_some());
    let b = replacement(&mut store, 2);
    assert_eq!(a.lineage(), b.lineage());
    assert!(paint(&mut cache, &b, now).is_some());
    cache.finish(b.serial(), Ok(Arc::new(texture(RgbaImage::new(1, 1)))));
    cache.begin(now);
    assert!(paint(&mut cache, &b, now).is_some());
    cache.begin(now);
    // The scene places A again: its own pixels must come back and stay.
    assert!(paint(&mut cache, &a, now).is_some());
    let own = Arc::new(texture(RgbaImage::new(1, 1)));
    cache.finish(a.serial(), Ok(own.clone()));
    cache.begin(now);
    for _ in 0..3 {
        assert!(matches!(cache.lookup(a.serial(), now), Lookup::Ready(_)));
        assert!(Arc::ptr_eq(&own, &paint(&mut cache, &a, now).unwrap()));
        cache.begin(now);
    }
}

#[test]
fn an_older_frame_placed_again_survives_two_paints_per_frame() {
    let (mut cache, mut store) = (ImageCache::default(), ImageStore::default());
    let now = Instant::now();
    let a = replacement(&mut store, 1);
    ready(&mut cache, a.serial(), 1, now);
    assert!(paint(&mut cache, &a, now).is_some());
    let b = replacement(&mut store, 2);
    assert!(paint(&mut cache, &b, now).is_some());
    cache
        .finished
        .lock()
        .unwrap()
        .push((b.serial(), Ok(RgbaImage::new(1, 1))));
    cache.begin(now);
    assert!(paint(&mut cache, &b, now).is_some());
    cache.begin(now); // the popup's paint in the same frame
    cache.begin(now);
    // The scene places A again; a popup is open, so each frame begins twice.
    assert!(paint(&mut cache, &a, now).is_some());
    cache.begin(now);
    cache
        .finished
        .lock()
        .unwrap()
        .push((a.serial(), Ok(RgbaImage::new(1, 1))));
    cache.begin(now);
    let Lookup::Ready(own) = cache.lookup(a.serial(), now) else {
        panic!("A's own decode was released");
    };
    for _ in 0..3 {
        assert!(Arc::ptr_eq(&own, &paint(&mut cache, &a, now).unwrap()));
        cache.begin(now);
        cache.begin(now);
    }
}
