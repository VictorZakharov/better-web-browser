//! Exact integer reduction of bounded 2x/4x native coverage.
//! Four adjacent output pixels are independent lanes; no SIMD/unsafe code or
//! dependency is needed. Empty groups skip arithmetic, useful for stroked rings.
use super::Crop;

pub(super) fn reduce<const SCALE: usize>(
    samples: &[u8],
    source_width: usize,
    crop: &Crop,
) -> Vec<u8> {
    debug_assert!(SCALE == 2 || SCALE == 4);
    let width = crop.width as usize;
    let stride = source_width * SCALE;
    let mut output = vec![0; width * crop.height as usize];
    for (y, destination) in output.chunks_exact_mut(width).enumerate() {
        let start = (y + crop.y as usize) * SCALE * stride + crop.x as usize * SCALE;
        let full = width / 4 * 4;
        for x in (0..full).step_by(4) {
            let mut lanes = 0u128;
            let mut nonzero = 0u128;
            for dy in 0..SCALE {
                let offset = start + dy * stride + x * SCALE;
                let packed = if SCALE == 4 {
                    u128::from_le_bytes(
                        samples[offset..offset + 16]
                            .try_into()
                            .expect("four 4x samples"),
                    )
                } else {
                    u128::from(u64::from_le_bytes(
                        samples[offset..offset + 8]
                            .try_into()
                            .expect("four 2x samples"),
                    ))
                };
                nonzero |= packed;
                // Each u16 lane contains two samples, at most 510 per row.
                const MASK: u128 = 0x00ff_00ff_00ff_00ff_00ff_00ff_00ff_00ff;
                lanes += (packed & MASK) + ((packed >> 8) & MASK);
            }
            if nonzero == 0 {
                continue;
            }
            if SCALE == 4 {
                // Collapse each pair of u16 lanes into a u32 output total.
                // Four rows sum to <=4080; no carry can cross a lane boundary.
                const PAIRS: u128 = 0x0000_ffff_0000_ffff_0000_ffff_0000_ffff;
                let sums = (lanes & PAIRS) + ((lanes >> 16) & PAIRS);
                for channel in 0..4 {
                    destination[x + channel] =
                        ((((sums >> (channel * 32)) & 0xffff) + 8) >> 4) as u8;
                }
            } else {
                for channel in 0..4 {
                    destination[x + channel] =
                        ((((lanes >> (channel * 16)) & 0xffff) + 2) >> 2) as u8;
                }
            }
        }
        for (x, pixel) in destination.iter_mut().enumerate().skip(full) {
            let mut sum = 0u16;
            for dy in 0..SCALE {
                let offset = start + dy * stride + x * SCALE;
                for value in &samples[offset..offset + SCALE] {
                    sum += u16::from(*value);
                }
            }
            *pixel = ((sum + (SCALE * SCALE / 2) as u16) / (SCALE * SCALE) as u16) as u8;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify<const SCALE: usize>() {
        for source_width in 1..=19 {
            let source_height = 8;
            let samples: Vec<_> = (0..source_width * source_height * SCALE * SCALE)
                .map(|i| {
                    if i % 13 < 5 {
                        0
                    } else {
                        (i * 73 + i / 11 * 19) as u8
                    }
                })
                .collect();
            for x in 0..source_width {
                for width in 1..=source_width - x {
                    for y in [0, 1, 3] {
                        let crop = Crop {
                            x: x as u32,
                            y: y as u32,
                            width: width as u32,
                            height: 5,
                        };
                        let actual = reduce::<SCALE>(&samples, source_width, &crop);
                        for row in 0..5 {
                            for column in 0..width {
                                let mut sum = 0u32;
                                for dy in 0..SCALE {
                                    for dx in 0..SCALE {
                                        sum += u32::from(
                                            samples[((row + y) * SCALE + dy)
                                                * source_width
                                                * SCALE
                                                + (column + x) * SCALE
                                                + dx],
                                        );
                                    }
                                }
                                let expected =
                                    (sum + (SCALE * SCALE / 2) as u32) / (SCALE * SCALE) as u32;
                                assert_eq!(
                                    u32::from(actual[row * width + column]),
                                    expected,
                                    "scale={SCALE}, source={source_width}, crop={x},{y},{width}, pixel={column},{row}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn packed_two_by_two_matches_independent_sums_for_every_crop_and_tail() {
        verify::<2>();
    }

    #[test]
    fn packed_four_by_four_matches_independent_sums_for_every_crop_and_tail() {
        verify::<4>();
    }

    #[test]
    fn maximum_samples_never_carry_into_adjacent_outputs() {
        let crop = Crop {
            x: 0,
            y: 0,
            width: 13,
            height: 5,
        };
        for value in [0, 1, 127, 128, 254, 255] {
            assert_eq!(
                reduce::<2>(&vec![value; 13 * 5 * 4], 13, &crop),
                vec![value; 13 * 5]
            );
            assert_eq!(
                reduce::<4>(&vec![value; 13 * 5 * 16], 13, &crop),
                vec![value; 13 * 5]
            );
        }
    }
}
