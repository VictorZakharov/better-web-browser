//! Opt-in native command samples; no browser, build or GPU suite overlaps timing.
use super::*;
use crate::windows_app::paint_primitives::fill_color_rect;
use std::time::Instant;

unsafe fn command(
    target: &Surface,
    source: &Surface,
    pixels: &DecodedImage,
    shared: Hdc,
    cull: bool,
) {
    let canvas = Rect {
        left: 0,
        top: 0,
        right: 640,
        bottom: 400,
    };
    fill_color_rect(target.dc, &canvas, 0x483726);
    for index in 0..354 {
        let destination = Rect {
            left: (index % 20) * 41,
            top: (index / 20) * 37,
            right: (index % 20) * 41 + 31,
            bottom: (index / 20) * 37 + 23,
        };
        if !cull || drawable(target.dc, destination) {
            paint_alpha_image(
                target.dc,
                shared,
                source.bitmap,
                pixels,
                &destination,
                false,
                None,
            );
        }
    }
    assert_ne!(GdiFlush(), 0);
}

#[test]
#[ignore = "manual sequential native command benchmark with byte-checked output"]
fn native_clipped_sprite_command_samples() {
    unsafe {
        let pixels = image(31, 23, false);
        let source = Surface::new(&pixels);
        source.detach();
        let reference = Surface::new(&image(640, 400, true));
        let candidate = Surface::new(&image(640, 400, true));
        let shared = CreateCompatibleDC(null_mut());
        assert!(!shared.is_null());
        for clipped in [false, true] {
            for target in [&reference, &candidate] {
                SelectClipRgn(target.dc, null_mut());
                if clipped {
                    IntersectClipRect(target.dc, 0, 0, 240, 140);
                }
            }
            command(&reference, &source, &pixels, null_mut(), false);
            command(&candidate, &source, &pixels, shared, true);
            assert_eq!(reference.bytes(), candidate.bytes());
            for round in 0..4 {
                for optimized in if round % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    for sample in 0..12 {
                        let started = Instant::now();
                        command(
                            if optimized { &candidate } else { &reference },
                            &source,
                            &pixels,
                            if optimized { shared } else { null_mut() },
                            optimized,
                        );
                        println!(
                            "native_sprite_command clipped={clipped} optimized={optimized} round={round} sample={sample} ms={:.4}",
                            started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                }
            }
            assert_eq!(reference.bytes(), candidate.bytes());
        }
        assert_ne!(DeleteDC(shared), 0);
    }
}
