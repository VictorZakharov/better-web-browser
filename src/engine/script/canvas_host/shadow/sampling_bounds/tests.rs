use super::*;

#[test]
fn support_contains_every_sample_admitted_by_the_original_full_surface_loop() {
    for width in [1, 7, 33, 128] {
        for height in [1, 9, 65] {
            for mask_width in [1, 11, 71] {
                for mask_height in [1, 19] {
                    for origin_x in [-400.0, -2.0, 0.0, 17.0, 120.0] {
                        for offset_x in [-f64::MAX, -123.25, -1.0, -0.5, 0.0, 0.25, 23.9, f64::MAX]
                        {
                            let origin_y = -3.0;
                            for offset_y in [-f64::MAX, -50.75, 0.0, 0.5, 63.0, f64::MAX] {
                                let [left, top, right, bottom] = translated_support(
                                    [width, height],
                                    [mask_width, mask_height],
                                    [origin_x, origin_y],
                                    [offset_x, offset_y],
                                );
                                assert!(left <= right && right <= width);
                                assert!(top <= bottom && bottom <= height);
                                for y in 0..height {
                                    for x in 0..width {
                                        let sx = f64::from(x) - offset_x - origin_x;
                                        let sy = f64::from(y) - offset_y - origin_y;
                                        if sx >= -1.0
                                            && sy >= -1.0
                                            && sx < f64::from(mask_width)
                                            && sy < f64::from(mask_height)
                                        {
                                            assert!(
                                                x >= left && x < right && y >= top && y < bottom
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn small_mask_prunes_large_texture_without_excluding_fractional_edges() {
    assert_eq!(
        translated_support([1024, 1024], [40, 20], [500.0, 600.0], [0.5, -0.25]),
        [498, 597, 542, 621]
    );
}
