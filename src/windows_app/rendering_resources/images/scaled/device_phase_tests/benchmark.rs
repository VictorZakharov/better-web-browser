//! Complete native draws; synchronization stays inside each retained sample.
use super::*;
use std::time::Instant;

unsafe fn draw(
    target: &DeviceTarget,
    source: &Bitmap,
    image: &DecodedImage,
    rect: Rect,
    cache: Option<&mut Cache>,
) {
    if let Some(cache) = cache {
        let plan =
            crate::windows_app::painting::affine::ScaledDestination::new(target.dc, rect).unwrap();
        let destination = plan.rect();
        if let Some(scaled) = cache.get(
            "device-phase",
            image,
            source.handle,
            &destination,
            target.dc,
        ) {
            crate::windows_app::paint_primitives::paint_alpha_bitmap_size(
                target.dc,
                scaled.handle,
                destination.width() as u32,
                destination.height() as u32,
                &destination,
            );
        } else {
            paint_alpha_bitmap(target.dc, source.handle, image, &destination);
        }
    } else {
        paint_alpha_bitmap(target.dc, source.handle, image, &rect);
    }
}

#[test]
#[ignore = "native timing experiment; run alone with --nocapture"]
fn measure_large_translucent_device_phase_draws() {
    unsafe {
        let source_image = patterned(1200, 800);
        let source = Bitmap::create(&source_image, &source_image.bgra, null_mut()).unwrap();
        let target = DeviceTarget::new(1605, 912);
        SetGraphicsMode(target.dc, GM_ADVANCED);
        let matrix = XFORM {
            eM11: 1.15,
            eM12: 0.0,
            eM21: 0.0,
            eM22: 1.15,
            eDx: 13.375,
            eDy: -7.125,
        };
        SetWorldTransform(target.dc, &matrix);
        let rect = Rect {
            left: -322,
            top: -183,
            right: 1926,
            bottom: 1094,
        };
        let mut cache = Cache::default();
        // Admission/allocation and warm-up are outside the repeated-command measurement.
        draw(&target, &source, &source_image, rect, Some(&mut cache));
        draw(&target, &source, &source_image, rect, Some(&mut cache));
        assert_eq!(cache.entries.len(), 1);
        let identity = XFORM {
            eM11: 1.0,
            eM12: 0.0,
            eM21: 0.0,
            eM22: 1.0,
            eDx: 0.0,
            eDy: 0.0,
        };
        let mut reference_pixels = None;
        for round in 0..4 {
            for cached in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                for sample in 0..15 {
                    SetWorldTransform(target.dc, &identity);
                    crate::windows_app::paint_primitives::fill_color_rect(
                        target.dc,
                        &Rect {
                            left: 0,
                            top: 0,
                            right: 1605,
                            bottom: 912,
                        },
                        0x332211,
                    );
                    SetWorldTransform(target.dc, &matrix);
                    GdiFlush();
                    let start = Instant::now();
                    for _ in 0..6 {
                        draw(
                            &target,
                            &source,
                            &source_image,
                            rect,
                            cached.then_some(&mut cache),
                        );
                    }
                    assert_ne!(GdiFlush(), 0);
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    let actual = target.pixels();
                    if let Some(expected) = &reference_pixels {
                        assert!(
                            expected == &actual,
                            "every retained native draw must preserve output pixels"
                        );
                    } else {
                        reference_pixels = Some(actual);
                    }
                    if sample >= 3 {
                        println!(
                            "native-device-phase round={round} cached={cached} sample={} ms={elapsed:.4}",
                            sample - 3
                        );
                    }
                }
            }
        }
    }
}
