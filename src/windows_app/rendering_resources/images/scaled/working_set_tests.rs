//! Repeated sprites must not defeat bounded admission just by cyclic order.
use super::super::tests::{image, pixels, surface::Target};
use super::tests::patterned;
use super::*;
use crate::windows_app::paint_primitives::{paint_alpha_bitmap, paint_alpha_bitmap_size};

const SPRITES: usize = 64;

fn destination(index: usize) -> Rect {
    let left = (index % 8 * 48) as i32;
    let top = (index / 8 * 32) as i32;
    Rect {
        left,
        top,
        right: left + 257,
        bottom: top + 129,
    }
}

unsafe fn frame(cache: &mut Cache, source: &Bitmap, image: &DecodedImage, target: &Target) {
    unsafe {
        for index in 0..SPRITES {
            let rect = destination(index);
            if let Some(scaled) = cache.get(
                &format!("sprite-{index}"),
                image,
                source.handle,
                &rect,
                target.dc,
            ) {
                paint_alpha_bitmap_size(
                    target.dc,
                    scaled.handle,
                    rect.width() as u32,
                    rect.height() as u32,
                    &rect,
                );
            } else {
                paint_alpha_bitmap(target.dc, source.handle, image, &rect);
            }
        }
        assert_ne!(GdiFlush(), 0);
    }
}

fn old_policy() -> Cache {
    Cache {
        entry_limit: 32,
        pending_limit: 32,
        ..Cache::default()
    }
}

#[test]
fn cyclic_working_set_admits_repeated_images_and_preserves_native_pixels() {
    unsafe {
        let image = patterned(97, 71);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        let original = Target::new(&super::super::tests::image(640, 400, [77; 4]));
        let candidate = Target::new(&super::super::tests::image(640, 400, [77; 4]));
        let mut reference = old_policy();
        let mut cache = Cache::default();
        for _ in 0..3 {
            frame(&mut reference, &source, &image, &original);
            frame(&mut cache, &source, &image, &candidate);
            assert_eq!(pixels(&original.bitmap), pixels(&candidate.bitmap));
            assert!(cache.bytes <= MAX_BYTES);
        }
        assert!(
            reference.entries.is_empty(),
            "32-key cyclic history never admits a repeated sprite"
        );
        assert_eq!(cache.entries.len(), SPRITES);
        assert!(cache.pending.is_empty());
        let handles = cache
            .entries
            .iter()
            .map(|v| v.bitmap.handle)
            .collect::<Vec<_>>();
        frame(&mut cache, &source, &image, &candidate);
        assert_eq!(
            handles,
            cache
                .entries
                .iter()
                .map(|v| v.bitmap.handle)
                .collect::<Vec<_>>()
        );
        cache.remove("sprite-0");
        assert_eq!(cache.entries.len(), SPRITES - 1);
        assert_eq!(
            cache.bytes,
            cache.entries.iter().map(|v| v.bitmap.bytes).sum::<usize>()
        );
    }
}

#[test]
fn admission_history_and_native_handles_remain_bounded_under_unique_keys() {
    unsafe {
        let image = image(97, 71, [24, 48, 72, 128]);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        let mut cache = Cache {
            entry_limit: 5,
            pending_limit: 9,
            ..Cache::default()
        };
        for index in 0..32 {
            let key = format!("unique-{index}");
            assert!(
                cache
                    .get(&key, &image, source.handle, &destination(0), null_mut())
                    .is_none()
            );
            assert!(
                cache
                    .get(&key, &image, source.handle, &destination(0), null_mut())
                    .is_some()
            );
            assert!(cache.entries.len() <= 5 && cache.bytes <= MAX_BYTES);
        }
        for index in 32..64 {
            assert!(
                cache
                    .get(
                        &format!("unique-{index}"),
                        &image,
                        source.handle,
                        &destination(0),
                        null_mut()
                    )
                    .is_none()
            );
            assert!(cache.pending.len() <= 9);
        }
        assert_eq!(cache.entries.len(), 5);
        assert_eq!(cache.pending.len(), 9);
        cache.clear();
        assert!(cache.entries.is_empty() && cache.pending.is_empty());
        assert_eq!(cache.bytes, 0);
    }
}

#[test]
fn oversized_data_url_keys_do_not_allocate_optional_cache_metadata() {
    unsafe {
        let image = image(97, 71, [24, 48, 72, 128]);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        let mut cache = Cache::default();
        let key = "x".repeat(MAX_SOURCE_KEY_BYTES + 1);
        for _ in 0..3 {
            assert!(
                cache
                    .get(&key, &image, source.handle, &destination(0), null_mut())
                    .is_none()
            );
        }
        assert!(cache.entries.is_empty() && cache.pending.is_empty());
        assert_eq!(cache.bytes, 0);
    }
}

#[test]
#[ignore = "windowless completed native-command timing; run alone, never in smoke CI"]
fn benchmark_cyclic_translucent_native_scaling() {
    unsafe {
        let image = patterned(97, 71);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        for round in 0..4 {
            let order = if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            };
            for candidate in order {
                let target = Target::new(&super::super::tests::image(640, 400, [77; 4]));
                let mut cache = if candidate {
                    Cache::default()
                } else {
                    old_policy()
                };
                for _ in 0..4 {
                    frame(&mut cache, &source, &image, &target);
                }
                let mut samples = Vec::new();
                for _ in 0..12 {
                    let start = std::time::Instant::now();
                    frame(&mut cache, &source, &image, &target);
                    samples.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                println!(
                    "round={round} policy={} completed_64_scaled_draws_ms={samples:?} admitted={} bytes={}",
                    if candidate {
                        "candidate"
                    } else {
                        "reference32"
                    },
                    cache.entries.len(),
                    cache.bytes
                );
                assert!(pixels(&target.bitmap).iter().any(|&byte| byte != 77));
            }
        }
    }
}
