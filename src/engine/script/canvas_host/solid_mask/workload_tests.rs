//! Repeated solid workloads retain byte-exact scalar semantics.
use super::*;

fn mask(pixels: usize, levels: usize) -> Vec<u8> {
    (0..pixels)
        .map(|index| match levels {
            1 => 255,
            3 => [0, 128, 255][index % 3],
            _ => (index % 256) as u8,
        })
        .collect()
}

#[test]
fn repeated_solid_workloads_preserve_scalar_bytes() {
    for pixels in [255, 256, 1024, 4096] {
        for levels in [1, 3, 256] {
            let coverage = mask(pixels, levels);
            for alpha in [0, 1, 127, 255] {
                for opacity in [0.0, 0.37, 1.0] {
                    let color = [31.5, 205.1, 73.25, 183.0];
                    let mut actual = [29, 63, 193, alpha].repeat(pixels);
                    let mut expected = actual.clone();
                    for (pixel, sample) in expected.chunks_exact_mut(4).zip(&coverage) {
                        if *sample != 0 {
                            source_over(pixel, color, opacity * (f64::from(*sample) / 255.0));
                        }
                    }
                    composite_in_place(Some(&coverage), &mut actual, color, opacity, None).unwrap();
                    assert_eq!(
                        actual, expected,
                        "pixels {pixels}, levels {levels}, alpha {alpha}"
                    );
                }
            }
        }
    }
}
