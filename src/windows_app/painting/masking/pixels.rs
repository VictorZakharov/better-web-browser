//! Exact RGB coverage multiplication; input mask RGB and destination alpha are ignored.
pub(super) fn apply(destination: &mut [u8], mask: &[u8]) {
    debug_assert_eq!(destination.len(), mask.len());
    let count = destination.len().min(mask.len()) / 4;
    #[cfg(target_arch = "x86_64")]
    // SSE2 is the x86-64 baseline. Both loads are unaligned and confined to
    // complete four-pixel blocks of independently owned slices.
    let index = unsafe { vector(destination, mask, count) };
    #[cfg(not(target_arch = "x86_64"))]
    let index = 0;
    for index in index..count {
        let offset = index * 4;
        let alpha = mask[offset + 3];
        for channel in &mut destination[offset..offset + 3] {
            *channel = ((u16::from(*channel) * u16::from(alpha) + 127) / 255) as u8;
        }
        destination[offset + 3] = alpha;
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn vector(destination: &mut [u8], mask: &[u8], count: usize) -> usize {
    use std::arch::x86_64::*;
    unsafe {
        let zero = _mm_setzero_si128();
        let rounding = _mm_set1_epi16(127);
        let one = _mm_set1_epi16(1);
        let rgb_bits = _mm_set1_epi32(0x00ff_ffff);
        let alpha_bits = _mm_set1_epi32(0xff00_0000u32 as i32);
        let divide = |value| {
            let rounded = _mm_add_epi16(value, rounding);
            _mm_srli_epi16(
                _mm_add_epi16(rounded, _mm_add_epi16(one, _mm_srli_epi16(rounded, 8))),
                8,
            )
        };
        let mut index = 0;
        while index + 4 <= count {
            let offset = index * 4;
            let source = _mm_loadu_si128(destination.as_ptr().add(offset).cast());
            let mask_pixels = _mm_loadu_si128(mask.as_ptr().add(offset).cast());
            let alpha = _mm_srli_epi32(mask_pixels, 24);
            let alpha_pair = _mm_or_si128(alpha, _mm_slli_epi32(alpha, 8));
            let repeated = _mm_or_si128(alpha_pair, _mm_slli_epi32(alpha_pair, 16));
            let low = divide(_mm_mullo_epi16(
                _mm_unpacklo_epi8(source, zero),
                _mm_unpacklo_epi8(repeated, zero),
            ));
            let high = divide(_mm_mullo_epi16(
                _mm_unpackhi_epi8(source, zero),
                _mm_unpackhi_epi8(repeated, zero),
            ));
            let rgb = _mm_and_si128(_mm_packus_epi16(low, high), rgb_bits);
            let result = _mm_or_si128(rgb, _mm_and_si128(mask_pixels, alpha_bits));
            _mm_storeu_si128(destination.as_mut_ptr().add(offset).cast(), result);
            index += 4;
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(destination: &mut [u8], mask: &[u8]) {
        for (pixel, mask) in destination.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * u16::from(mask[3]) + 127) / 255) as u8;
            }
            pixel[3] = mask[3];
        }
    }

    #[test]
    fn every_channel_alpha_pair_matches_scalar_rounding_and_ignores_other_alpha_rgb() {
        let mut destination = Vec::new();
        let mut mask = Vec::new();
        for channel in 0..=255u8 {
            for alpha in 0..=255u8 {
                destination.extend([channel, channel, channel, channel ^ alpha]);
                mask.extend([channel ^ 0xa5, 93, 247, alpha]);
            }
        }
        let mut expected = destination.clone();
        reference(&mut expected, &mask);
        apply(&mut destination, &mask);
        assert_eq!(destination, expected);
    }

    #[test]
    fn arbitrary_alignments_short_tails_and_empty_slices_preserve_sentinels() {
        for offset in 0..16 {
            for pixels in 0..37 {
                let bytes = pixels * 4 + 3;
                let mut destination = (0..offset + bytes + 16)
                    .map(|i| (i * 37) as u8)
                    .collect::<Vec<_>>();
                let mask = (0..offset + bytes + 16)
                    .map(|i| (i * 73 + 19) as u8)
                    .collect::<Vec<_>>();
                let mut expected = destination.clone();
                reference(&mut expected[offset..offset + bytes], &mask[15..15 + bytes]);
                apply(
                    &mut destination[offset..offset + bytes],
                    &mask[15..15 + bytes],
                );
                assert_eq!(destination, expected, "offset={offset}, pixels={pixels}");
            }
        }
        apply(&mut [], &[]);
    }
}
