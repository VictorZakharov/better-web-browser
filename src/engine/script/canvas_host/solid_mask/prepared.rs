//! Hoist solid-source arithmetic out of the destination loop. This preserves
//! the existing scalar equation, including its byte rounding at coverage edges.
//! Coverage and clip remain separate: no shader or geometry result is cached.

struct Sample {
    alpha: f64,
    remainder: f64,
    rgb: [f64; 3],
    transparent: [u8; 4],
}

pub(super) struct Paint {
    samples: [Sample; 256],
}

impl Paint {
    pub(super) fn new(color: [f64; 4], opacity: f64) -> Self {
        Self {
            samples: std::array::from_fn(|coverage| {
                // Keep operation order identical to the pre-existing scalar
                // path. Reassociating opacity/coverage changes half-byte ties.
                let opacity = opacity * (coverage as f64 / 255.0);
                let alpha = color[3] / 255.0 * opacity;
                let mut transparent = [0; 4];
                super::source_over(&mut transparent, color, opacity);
                Sample {
                    alpha,
                    remainder: 1.0 - alpha,
                    rgb: std::array::from_fn(|index| alpha * color[index]),
                    transparent,
                }
            }),
        }
    }

    pub(super) fn apply(&self, pixel: &mut [u8], coverage: u8) {
        let sample = &self.samples[coverage as usize];
        if sample.alpha == 1.0 || pixel[3] == 0 {
            pixel.copy_from_slice(&sample.transparent);
            return;
        }
        let backdrop = f64::from(pixel[3]) / 255.0 * sample.remainder;
        let alpha = sample.alpha + backdrop;
        if alpha == 0.0 {
            pixel.fill(0);
        } else if sample.alpha != 0.0 {
            for (index, value) in pixel[..3].iter_mut().enumerate() {
                *value = ((sample.rgb[index] + backdrop * f64::from(*value)) / alpha).round() as u8;
            }
            pixel[3] = (alpha * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_samples_match_scalar_for_every_coverage_and_backdrop_alpha() {
        for source_alpha in [0.0, 0.5, 1.0, 63.5, 127.0, 128.5, 254.0, 255.0] {
            for opacity in [0.0, 0.0001, 0.125, 0.37, 0.5, 1.0] {
                for rgb in [
                    [0.0, 128.5, 255.0],
                    [31.5, 63.5, 254.5],
                    [12.34, 101.01, 9.99],
                ] {
                    let color = [rgb[0], rgb[1], rgb[2], source_alpha];
                    let paint = Paint::new(color, opacity);
                    for coverage in 0..=255 {
                        for alpha in 0..=255 {
                            let mut actual = [19, 91, 213, alpha];
                            let mut expected = actual;
                            paint.apply(&mut actual, coverage);
                            super::super::source_over(
                                &mut expected,
                                color,
                                opacity * (f64::from(coverage) / 255.0),
                            );
                            assert_eq!(
                                actual, expected,
                                "color {color:?}, opacity {opacity}, coverage {coverage}, backdrop {alpha}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn transparent_source_does_not_destroy_nontransparent_backdrop_rgb() {
        let paint = Paint::new([200.0, 40.0, 80.0, 0.0], 1.0);
        for coverage in [0, 1, 127, 255] {
            let mut pixel = [19, 91, 213, 128];
            paint.apply(&mut pixel, coverage);
            assert_eq!(pixel, [19, 91, 213, 128]);
        }
    }
}
