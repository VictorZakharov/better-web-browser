use super::*;

fn original_pixel(pixel: &mut [u8], color: [f64; 4], opacity: f64, coverage: u8) {
    if coverage == 0 {
        return;
    }
    let source_alpha = color[3] / 255.0 * (opacity * (f64::from(coverage) / 255.0));
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

#[test]
fn sparse_masks_and_odd_tails_equal_the_original_scalar_equation() {
    for pixels in 1..=129 {
        for phase in 0..=17 {
            let mask: Vec<u8> = (0..pixels)
                .map(|i| {
                    if (i + phase) % 37 < 30 {
                        0
                    } else {
                        (i * 73 + phase * 11) as u8
                    }
                })
                .collect();
            for color in [
                [0.0; 4],
                [19.5, 250.49, 127.5, 255.0],
                [255.0, 0.0, 15.0, 117.0],
            ] {
                for opacity in [0.0, 0.25, 1.0] {
                    let destination: Vec<u8> = (0..pixels * 4).map(|i| (i * 43) as u8).collect();
                    let mut expected = destination.clone();
                    for (pixel, coverage) in expected.chunks_exact_mut(4).zip(&mask) {
                        original_pixel(pixel, color, opacity, *coverage);
                    }
                    assert_eq!(
                        composite(&mask, &destination, color, opacity).unwrap(),
                        expected,
                        "pixels={pixels}, phase={phase}, opacity={opacity}"
                    );
                }
            }
        }
    }
}

#[test]
fn sparse_masks_keep_whole_bitmap_clip_coordinates_and_odd_region_strides() {
    let (bitmap_width, bitmap_height) = (37_usize, 13_usize);
    let bits: Vec<u8> = (0..(bitmap_width * bitmap_height).div_ceil(8))
        .map(|i| (i * 53 + 17) as u8)
        .collect();
    let (left, top) = (3, 2);
    for width in [5_usize, 8, 17, 31] {
        let height = 7;
        let args = vec![
            JsValue::Bytes(bits.clone()),
            JsValue::from(bitmap_width as f64),
            JsValue::from(bitmap_height as f64),
            JsValue::from(left as f64),
            JsValue::from(top as f64),
        ];
        let clip = super::super::raster_clip::Clip::from_args(&args, 0, [width as u32, height])
            .unwrap()
            .unwrap();
        let mask: Vec<u8> = (0..width * height as usize)
            .map(|i| if i % 29 < 22 { 0 } else { 128 })
            .collect();
        let destination = [9, 23, 201, 57].repeat(mask.len());
        let color = [217.0, 18.5, 94.0, 183.0];
        let mut expected = destination.clone();
        for (index, pixel) in expected.chunks_exact_mut(4).enumerate() {
            let bit = (index / width + top) * bitmap_width + index % width + left;
            if bits[bit / 8] & (1 << (bit % 8)) != 0 {
                original_pixel(pixel, color, 0.5, mask[index]);
            }
        }
        assert_eq!(
            composite_clipped_region(Some(&mask), &destination, color, 0.5, Some(&clip)).unwrap(),
            expected
        );
    }
}
