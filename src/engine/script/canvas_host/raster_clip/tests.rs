use super::*;

fn args(bits: Vec<u8>, width: f64, height: f64, left: f64, top: f64) -> Vec<JsValue> {
    vec![
        JsValue::Bytes(bits),
        JsValue::from(width),
        JsValue::from(height),
        JsValue::from(left),
        JsValue::from(top),
    ]
}

#[test]
fn odd_width_rows_use_whole_bitmap_bit_offsets_not_region_local_alignment() {
    let bytes = [0b1010_1101, 0b0101_0011, 0b0000_1110];
    for width in 1_usize..=7 {
        let pixels = width * 3;
        let input = args(
            bytes[..pixels.div_ceil(8)].to_vec(),
            width as f64,
            3.0,
            0.0,
            0.0,
        );
        let clip = Clip::from_args(&input, 0, [width as u32, 3])
            .unwrap()
            .unwrap();
        for index in 0..pixels {
            assert_eq!(
                clip.allows(index),
                bytes[index / 8] & (1 << (index % 8)) != 0
            );
        }
    }
}

#[test]
fn translated_region_respects_sparse_holes_and_last_partial_byte() {
    let bits = [0b1100_1010, 0b0110_1101, 0b1110_0011, 0b0000_0101];
    for left in 0..=4 {
        for top in 0..=2 {
            let input = args(bits.to_vec(), 7.0, 4.0, f64::from(left), f64::from(top));
            let clip = Clip::from_args(&input, 0, [3, 2]).unwrap().unwrap();
            for index in 0..6 {
                let global = (index / 3 + top as usize) * 7 + index % 3 + left as usize;
                assert_eq!(
                    clip.allows(index),
                    bits[global / 8] & (1 << (global % 8)) != 0
                );
            }
        }
    }
}

#[test]
fn missing_null_or_malformed_clip_never_becomes_an_all_painted_region() {
    assert!(Clip::from_args(&[], 0, [3, 2]).unwrap().is_none());
    assert!(
        Clip::from_args(&[JsValue::Null], 0, [3, 2])
            .unwrap()
            .is_none()
    );
    for input in [
        args(vec![255], 7.0, 4.0, 0.0, 0.0),
        args(vec![255; 4], 7.0, 4.0, 5.0, 0.0),
        args(vec![255; 4], 7.0, 4.0, 0.0, 3.0),
        args(vec![255; 4], 7.0, 4.0, -1.0, 0.0),
        args(vec![255; 4], 7.0, 4.0, 0.5, 0.0),
        args(vec![255; 4], 7.0, 4.0, f64::NAN, 0.0),
        args(vec![255; 4], 7.0, 4.0, 0.0, f64::INFINITY),
    ] {
        assert!(Clip::from_args(&input, 0, [3, 2]).is_none());
    }
}

#[test]
fn clipped_owned_compositor_matches_scalar_clip_admission_and_preserves_inputs() {
    for alpha in [0, 1, 128, 255] {
        for opacity in [0.0, 0.25, 0.5, 1.0] {
            let input = args(vec![0b1100_1010, 0b0110_1101], 5.0, 3.0, 1.0, 1.0);
            let clip = Clip::from_args(&input, 0, [3, 2]).unwrap().unwrap();
            let destination = [17, 31, 123, alpha].repeat(6);
            let mask = [0, 1, 127, 128, 254, 255];
            let color = [200.0, 40.5, 80.0, 191.0];
            let mut expected = destination.clone();
            for (index, pixel) in expected.chunks_exact_mut(4).enumerate() {
                if mask[index] != 0 && clip.allows(index) {
                    super::super::solid_mask::source_over(
                        pixel,
                        color,
                        opacity * f64::from(mask[index]) / 255.0,
                    );
                }
            }
            let actual = super::super::solid_mask::composite_clipped_region(
                Some(&mask),
                &destination,
                color,
                opacity,
                Some(&clip),
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(destination, [17, 31, 123, alpha].repeat(6));
        }
    }
}
