//! Batched source-over arithmetic for an owned, bounded Canvas raster region.
//! Geometry coverage remains separate from source alpha. This is the same
//! compositing equation as the scalar JS path, not a second rasterizer.

use super::*;

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let mask = match args.get(1) {
        Some(JsValue::Null) => None,
        Some(value) => match value.as_bytes() {
            Some(bytes) => Some(bytes),
            None => return JsValue::Null,
        },
        None => return JsValue::Null,
    };
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let Some(JsValue::Array(channels)) = args.get(3) else {
        return JsValue::Null;
    };
    let Some(opacity) = args.get(4).and_then(JsValue::as_number) else {
        return JsValue::Null;
    };
    let mut color = [0.0; 4];
    if channels.len() != color.len() {
        return JsValue::Null;
    }
    for (output, input) in color.iter_mut().zip(channels) {
        let Some(value) = input.as_number() else {
            return JsValue::Null;
        };
        *output = value;
    }
    let clip = match args.get(5) {
        None | Some(JsValue::Null) => None,
        Some(_) => {
            let Some(width) = args.get(10).and_then(super::source_layer::dimension) else {
                return JsValue::Null;
            };
            let Some(height) = args.get(11).and_then(super::source_layer::dimension) else {
                return JsValue::Null;
            };
            if width as usize * height as usize != destination.len() / 4 {
                return JsValue::Null;
            }
            let Some(clip) = super::raster_clip::Clip::from_args(args, 5, [width, height]) else {
                return JsValue::Null;
            };
            clip
        }
    };
    composite_clipped_region(mask, destination, color, opacity, clip.as_ref())
        .map_or(JsValue::Null, JsValue::Bytes)
}

#[cfg(test)]
fn composite(mask: &[u8], destination: &[u8], color: [f64; 4], opacity: f64) -> Option<Vec<u8>> {
    composite_region(Some(mask), destination, color, opacity)
}

#[cfg(test)]
pub(super) fn composite_region(
    mask: Option<&[u8]>,
    destination: &[u8],
    color: [f64; 4],
    opacity: f64,
) -> Option<Vec<u8>> {
    composite_clipped_region(mask, destination, color, opacity, None)
}

pub(super) fn composite_clipped_region(
    mask: Option<&[u8]>,
    destination: &[u8],
    color: [f64; 4],
    opacity: f64,
    clip: Option<&super::raster_clip::Clip<'_>>,
) -> Option<Vec<u8>> {
    let pixels = destination.len() / 4;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || !destination.len().is_multiple_of(4)
        || mask.is_some_and(|mask| mask.len() != pixels)
        || !opacity.is_finite()
        || !(0.0..=1.0).contains(&opacity)
        || color
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=255.0).contains(v))
    {
        return None;
    }
    let mut output = destination.to_vec();
    for (index, pixel) in output.chunks_exact_mut(4).enumerate() {
        // An aligned integer rectangle has complete coverage. Do not allocate
        // a redundant all-255 mask just to move its paint off the JS pixel loop.
        let coverage = mask.map_or(255, |mask| mask[index]);
        if coverage == 0 || clip.is_some_and(|clip| !clip.allows(index)) {
            continue;
        }
        source_over(pixel, color, opacity * (f64::from(coverage) / 255.0));
    }
    Some(output)
}

pub(super) fn source_over(pixel: &mut [u8], color: [f64; 4], opacity: f64) {
    let source_alpha = color[3] / 255.0 * opacity;
    if source_alpha == 1.0 {
        // Fully covered opaque source-over replaces the backdrop. Keep the
        // same round() conversion as the scalar equation (including fractional
        // channels), without its redundant destination load/divisions.
        for channel in 0..3 {
            pixel[channel] = color[channel].round() as u8;
        }
        pixel[3] = 255;
        return;
    }
    let backdrop_weight = f64::from(pixel[3]) / 255.0 * (1.0 - source_alpha);
    let output_alpha = source_alpha + backdrop_weight;
    if output_alpha == 0.0 {
        pixel.fill(0);
    } else if source_alpha != 0.0 {
        for channel in 0..3 {
            pixel[channel] = ((source_alpha * color[channel]
                + backdrop_weight * f64::from(pixel[channel]))
                / output_alpha)
                .round() as u8;
        }
        pixel[3] = (output_alpha * 255.0).round() as u8;
    }
}

#[cfg(test)]
#[path = "solid_mask/sparse_tests.rs"]
mod sparse_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_replacement_matches_the_original_equation_for_every_backdrop_alpha() {
        for backdrop_alpha in 0..=255 {
            for channel in [0.0, 0.49, 0.5, 1.5, 63.5, 127.5, 254.5, 255.0] {
                let color = [channel, 255.0 - channel, channel / 2.0, 255.0];
                let mut actual = [19, 91, 213, backdrop_alpha];
                source_over(&mut actual, color, 1.0);
                let source_alpha = color[3] / 255.0;
                let backdrop_weight = f64::from(backdrop_alpha) / 255.0 * (1.0 - source_alpha);
                let output_alpha = source_alpha + backdrop_weight;
                let mut expected = [19, 91, 213, backdrop_alpha];
                for index in 0..3 {
                    expected[index] = ((source_alpha * color[index]
                        + backdrop_weight * f64::from(expected[index]))
                        / output_alpha)
                        .round() as u8;
                }
                expected[3] = (output_alpha * 255.0).round() as u8;
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn complete_rectangles_equal_full_coverage_masks_without_allocating_one() {
        for alpha in [0, 1, 64, 127, 128, 254, 255] {
            for opacity in [0.0, 0.125, 0.5, 1.0] {
                let destination = [20, 60, 140, alpha].repeat(257);
                let color = [200.0, 40.0, 80.0, 128.0];
                assert_eq!(
                    composite_region(None, &destination, color, opacity),
                    composite(&[255; 257], &destination, color, opacity)
                );
            }
        }
        assert!(composite_region(None, &[], [0.0; 4], 1.0).is_none());
        assert!(composite_region(None, &[0; 7], [0.0; 4], 1.0).is_none());
    }

    #[test]
    fn complete_coverage_and_partial_coverage_apply_source_alpha_once() {
        let mask = [0, 128, 255];
        let pixels = [0; 12];
        assert_eq!(
            composite(&mask, &pixels, [255.0, 0.0, 0.0, 255.0], 0.5).unwrap(),
            [0, 0, 0, 0, 255, 0, 0, 64, 255, 0, 0, 128]
        );
        assert_eq!(pixels, [0; 12]);
    }

    #[test]
    fn alpha_matches_independent_integer_oracle_over_all_coverage_values() {
        let mask: Vec<_> = (0..=255).collect();
        for backdrop_alpha in [0, 1, 64, 127, 128, 254, 255] {
            let pixels = [0, 0, 255, backdrop_alpha].repeat(256);
            let actual = composite(&mask, &pixels, [255.0, 0.0, 0.0, 255.0], 1.0).unwrap();
            for (coverage, pixel) in actual.chunks_exact(4).enumerate() {
                let expected = (coverage as u32 * 255
                    + u32::from(backdrop_alpha) * (255 - coverage as u32)
                    + 127)
                    / 255;
                assert_eq!(u32::from(pixel[3]), expected);
            }
        }
    }

    #[test]
    fn color_is_normalized_after_premultiplied_contributions_are_combined() {
        assert_eq!(
            composite(&[255], &[0, 0, 255, 128], [255.0, 0.0, 0.0, 255.0], 0.5).unwrap(),
            [170, 0, 85, 192]
        );
        assert_eq!(
            composite(&[255], &[0, 0, 255, 255], [255.0, 0.0, 0.0, 128.0], 1.0).unwrap(),
            [128, 0, 127, 255]
        );
    }

    #[test]
    fn uncovered_hidden_rgb_is_untouched_but_covered_transparent_output_is_black() {
        assert_eq!(
            composite(&[0, 255], &[7, 8, 9, 0, 7, 8, 9, 0], [0.0; 4], 1.0).unwrap(),
            [7, 8, 9, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn invalid_dimensions_colors_opacity_and_bridge_shapes_decline_atomically() {
        assert!(composite(&[], &[], [0.0; 4], 1.0).is_none());
        assert!(composite(&[255], &[0; 3], [0.0; 4], 1.0).is_none());
        assert!(composite(&vec![0; MAX_CANVAS_PIXELS + 1], &[], [0.0; 4], 1.0).is_none());
        for opacity in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert!(composite(&[255], &[0; 4], [0.0; 4], opacity).is_none());
        }
        for channel in [f64::NAN, f64::INFINITY, -1.0, 256.0] {
            assert!(composite(&[255], &[0; 4], [channel, 0.0, 0.0, 255.0], 1.0).is_none());
        }
        assert_eq!(paint(&[]), JsValue::Null);
        assert_eq!(
            paint(&[
                JsValue::from("canvasPaintSolidMask".to_string()),
                JsValue::Bytes(vec![255]),
                JsValue::Bytes(vec![0; 4]),
                JsValue::Array(vec![JsValue::from(0.0); 3]),
                JsValue::from(1.0),
            ]),
            JsValue::Null
        );
    }
}
