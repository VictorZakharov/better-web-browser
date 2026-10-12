//! Opt-in command timings; output is checked before and after every sample round.
use super::*;
use std::time::Instant;

unsafe fn frame(
    target: &DeviceTarget,
    image: &DecodedImage,
    source: &Bitmap,
    cache: &mut Cache,
    optimized: bool,
) {
    let identity = crate::windows_app::painting::affine::IdentityScope::new(target.dc).unwrap();
    crate::windows_app::paint_primitives::fill_color_rect(
        target.dc,
        &Rect {
            left: 0,
            top: 0,
            right: 640,
            bottom: 400,
        },
        0x332211,
    );
    drop(identity);
    for index in 0..32 {
        let rect = Rect {
            left: index % 8 * 45,
            top: index / 8 * 38,
            right: index % 8 * 45 + 257,
            bottom: index / 8 * 38 + 129,
        };
        let plan = Reflection::new(target.dc, rect).unwrap();
        if optimized
            && let Some(baked) = cache.reflected(
                &format!("sprite-{index}"),
                image,
                source.handle,
                plan,
                target.dc,
            )
        {
            let identity =
                crate::windows_app::painting::affine::IdentityScope::new(target.dc).unwrap();
            let bounds = plan.bounds();
            paint_alpha_bitmap_size(
                target.dc,
                baked.handle,
                bounds.width() as u32,
                bounds.height() as u32,
                &bounds,
            );
            drop(identity);
        } else {
            paint_alpha_bitmap(target.dc, source.handle, image, &rect);
        }
    }
    assert_ne!(GdiFlush(), 0);
}

#[test]
#[ignore = "manual sequential release native-reflection command samples"]
fn native_horizontal_reflection_command_samples() {
    unsafe {
        let image = patterned(97, 71);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        let expected = DeviceTarget::new(640, 400);
        let actual = DeviceTarget::new(640, 400);
        for target in [&expected, &actual] {
            setup(target.dc, [-1.25, 0.0, 0.0, 1.25, 613.125, 17.75], false);
        }
        let mut unused = Cache::default();
        let mut cache = Cache::default();
        for _ in 0..3 {
            frame(&expected, &image, &source, &mut unused, false);
            frame(&actual, &image, &source, &mut cache, true);
            assert_eq!(actual.pixels(), expected.pixels());
        }
        assert_eq!(cache.entries.len(), 32);
        for round in 0..4 {
            for optimized in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                for sample in 0..12 {
                    let started = Instant::now();
                    frame(
                        if optimized { &actual } else { &expected },
                        &image,
                        &source,
                        if optimized { &mut cache } else { &mut unused },
                        optimized,
                    );
                    println!(
                        "native_reflection_command optimized={optimized} round={round} sample={sample} ms={:.4}",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
            }
            assert_eq!(actual.pixels(), expected.pixels());
        }
    }
}
