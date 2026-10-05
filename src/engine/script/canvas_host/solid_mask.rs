//! Batched source-over arithmetic for an owned, bounded Canvas raster region.
//! Geometry coverage remains separate from source alpha. This is the same
//! compositing equation as the scalar JS path, not a second rasterizer.

use super::*;

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(mask) = args.get(1).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
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
    composite(mask, destination, color, opacity).map_or(JsValue::Null, JsValue::Bytes)
}

fn composite(mask: &[u8], destination: &[u8], color: [f64; 4], opacity: f64) -> Option<Vec<u8>> {
    if mask.is_empty()
        || mask.len() > MAX_CANVAS_PIXELS
        || mask.len().checked_mul(4)? != destination.len()
        || !opacity.is_finite()
        || !(0.0..=1.0).contains(&opacity)
        || color
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=255.0).contains(v))
    {
        return None;
    }
    let mut output = destination.to_vec();
    for (pixel, coverage) in output.chunks_exact_mut(4).zip(mask) {
        if *coverage == 0 {
            continue;
        }
        source_over(pixel, color, opacity * (f64::from(*coverage) / 255.0));
    }
    Some(output)
}

pub(super) fn source_over(pixel: &mut [u8], color: [f64; 4], opacity: f64) {
    let source_alpha = color[3] / 255.0 * opacity;
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
mod tests {
    use super::*;

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
