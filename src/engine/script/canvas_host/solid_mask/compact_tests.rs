//! Compact coverage is storage only: complete byte oracles include hidden RGB
//! and packed clips whose origin/row stride differ from the painted region.
use super::super::coverage_storage::Coverage;
use super::*;

#[test]
fn compact_native_paint_equals_dense_for_clips_alpha_and_every_storage_choice() {
    let width = 64usize;
    let height = 65usize;
    let pixels = width * height;
    for shape in 0..4 {
        let mask: Vec<_> = (0..pixels)
            .map(|index| match shape {
                0 => 0,
                1 if index % width < 3 => [1, 128, 255][index % width],
                2 => (index % 256) as u8,
                3 => 255,
                _ => 0,
            })
            .collect();
        let stored = Coverage::new(mask.clone());
        for clip_kind in 0..4 {
            let clip_bits: Vec<_> = (0..(96usize * 96).div_ceil(8))
                .map(|index| match clip_kind {
                    0 | 1 => 255,
                    2 => 0,
                    _ => (index * 73 + 19) as u8,
                })
                .collect();
            let arguments = [
                JsValue::Bytes(clip_bits),
                JsValue::from(96),
                JsValue::from(96),
                JsValue::from(17),
                JsValue::from(9),
            ];
            let clip = if clip_kind == 0 {
                None
            } else {
                super::super::raster_clip::Clip::from_args(
                    &arguments,
                    0,
                    [width as u32, height as u32],
                )
                .unwrap()
            };
            for alpha in [0, 1, 127, 255] {
                for source_alpha in [0.0, 0.5, 128.0, 255.0] {
                    for opacity in [0.0, 0.37, 1.0] {
                        let color = [31.5, 205.1, 73.25, source_alpha];
                        let mut actual = [29, 63, 193, alpha].repeat(pixels);
                        let mut expected = actual.clone();
                        composite_in_place(
                            Some(&mask),
                            &mut expected,
                            color,
                            opacity,
                            clip.as_ref(),
                        )
                        .unwrap();
                        composite_coverage_in_place(
                            &stored,
                            &mut actual,
                            color,
                            opacity,
                            clip.as_ref(),
                        )
                        .unwrap();
                        assert_eq!(
                            actual, expected,
                            "shape {shape}, clip {clip_kind}, backdrop {alpha}, source {source_alpha}, opacity {opacity}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_compact_paint_declines_before_changing_destination() {
    let mask = Coverage::new(vec![255; 1024]);
    for bytes in [0, 3, 1023 * 4, 1024 * 4 + 1, 1025 * 4] {
        let mut destination = vec![13; bytes];
        let before = destination.clone();
        assert!(
            composite_coverage_in_place(&mask, &mut destination, [255.0; 4], 1.0, None).is_none()
        );
        assert_eq!(destination, before);
    }
    for (color, opacity) in [
        ([255.0; 4], f64::NAN),
        ([255.0; 4], -1.0),
        ([f64::INFINITY; 4], 1.0),
        ([-1.0; 4], 1.0),
    ] {
        let mut destination = vec![13; 1024 * 4];
        let before = destination.clone();
        assert!(
            composite_coverage_in_place(&mask, &mut destination, color, opacity, None).is_none()
        );
        assert_eq!(destination, before);
    }
}
