//! The optimized diagonal sampler is byte-identical to the general inverse path.
use super::*;
use crate::windows_app::platform::Rect;
use better_web_browser::engine::{DecodedImage, affine::Affine2D};

fn mask(width: u32, height: u32) -> DecodedImage {
    DecodedImage {
        width,
        height,
        bgra: (0..width as usize * height as usize * 4)
            .map(|i| (i * 73 + 19) as u8)
            .collect::<Vec<_>>()
            .into(),
    }
}

fn pixels(width: usize, height: usize) -> Vec<u8> {
    (0..width * height * 4)
        .map(|i| (i * 37 + 53) as u8)
        .collect()
}

#[test]
fn diagonal_sampling_matches_general_inverse_with_reflections_fractional_phases_and_clips() {
    let full = Rect {
        left: -17,
        top: 11,
        right: 43,
        bottom: 38,
    };
    for [sx, sy] in [
        [1.0, 1.0],
        [1.25, 1.5],
        [-1.3, 1.3],
        [0.375, -2.0],
        [-0.9, -0.7],
    ] {
        for [dx, dy] in [[0.0, 0.0], [33.125, -19.75], [-30.5, 27.3]] {
            let matrix = Affine2D([sx, 0.0, 0.0, sy, dx, dy]);
            for size in [[1, 1], [2, 7], [60, 27], [97, 71]] {
                let image = mask(size[0], size[1]);
                for rect in [
                    Rect {
                        left: -83,
                        top: -71,
                        right: 114,
                        bottom: 98,
                    },
                    Rect {
                        left: -9,
                        top: 14,
                        right: 33,
                        bottom: 29,
                    },
                    Rect {
                        left: 14,
                        top: 7,
                        right: 15,
                        bottom: 8,
                    },
                ] {
                    let width = rect.width() as usize;
                    let mut actual = pixels(width, rect.height() as usize);
                    let mut expected = actual.clone();
                    general_affine(
                        &mut expected,
                        rect,
                        full,
                        &image,
                        matrix.inverse().unwrap(),
                        width,
                    );
                    apply_affine(&mut actual, rect, full, &image, matrix);
                    assert_eq!(
                        actual,
                        expected,
                        "matrix={matrix:?}, mask={size:?}, clip={:?}",
                        [rect.left, rect.top, rect.right, rect.bottom]
                    );
                }
            }
        }
    }
}

#[test]
fn general_rotation_shear_and_singular_fallback_keep_existing_pixels() {
    let rect = Rect {
        left: -12,
        top: 5,
        right: 47,
        bottom: 39,
    };
    let image = mask(31, 23);
    for matrix in [
        Affine2D([0.8, 0.4, -0.4, 0.8, 12.1, -3.7]),
        Affine2D([1.0, 0.0, 0.25, 1.0, -2.1, 0.0]),
        Affine2D([1.0, 0.25, 0.0, 1.0, -2.1, 0.0]),
    ] {
        let mut actual = pixels(rect.width() as usize, rect.height() as usize);
        let mut expected = actual.clone();
        general_affine(
            &mut expected,
            rect,
            rect,
            &image,
            matrix.inverse().unwrap(),
            rect.width() as usize,
        );
        apply_affine(&mut actual, rect, rect, &image, matrix);
        assert_eq!(actual, expected);
    }
    let mut actual = vec![127; 59 * 34 * 4];
    apply_affine(&mut actual, rect, rect, &image, Affine2D([0.0; 6]));
    assert!(actual.iter().all(|byte| *byte == 0));
}

#[test]
#[ignore = "isolated transformed mask command; run alone without heavy builds or suites"]
fn benchmark_diagonal_mask_sampling() {
    let rect = Rect {
        left: 0,
        top: 0,
        right: 1605,
        bottom: 912,
    };
    let full = Rect {
        left: -322,
        top: -183,
        right: 1926,
        bottom: 1094,
    };
    let image = mask(1200, 800);
    let matrix = Affine2D([-1.3, 0.0, 0.0, 1.3, 1613.125, -51.75]);
    let inverse = matrix.inverse().unwrap();
    let input = pixels(rect.width() as usize, rect.height() as usize);
    let mut expected = input.clone();
    general_affine(
        &mut expected,
        rect,
        full,
        &image,
        inverse,
        rect.width() as usize,
    );
    for round in 0..4 {
        for hoisted in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut samples = Vec::new();
            for sample in 0..15 {
                let mut result = input.clone();
                let start = std::time::Instant::now();
                if hoisted {
                    apply_affine(&mut result, rect, full, &image, matrix);
                } else {
                    general_affine(
                        &mut result,
                        rect,
                        full,
                        &image,
                        inverse,
                        rect.width() as usize,
                    );
                }
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert!(
                    result == expected,
                    "changed mask coverage or premultiplication"
                );
                if sample >= 3 {
                    samples.push(elapsed);
                }
            }
            println!("round={round} hoisted={hoisted} completed_1605x912_mask_ms={samples:?}");
        }
    }
}
