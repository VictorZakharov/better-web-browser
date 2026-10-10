//! Exact straight RGBA8 to associated-alpha BGRA8 presentation conversion.
pub(crate) fn rgba_to_bgra(pixels: &mut [u8]) {
    #[cfg(target_arch = "x86_64")]
    {
        // SSE2 is part of the x86-64 baseline. No CPU dispatch or stronger ISA is required.
        unsafe { vectorized(pixels) };
    }
    #[cfg(not(target_arch = "x86_64"))]
    scalar(pixels);
}

#[cfg(target_arch = "x86_64")]
unsafe fn vectorized(pixels: &mut [u8]) {
    use std::arch::x86_64::*;

    let (blocks, tail) = pixels.as_chunks_mut::<16>();
    // SAFETY: every load/store addresses one complete 16-byte block in the uniquely
    // borrowed slice. Unaligned operations support arbitrary bitmap row alignment.
    // All intrinsics below require only SSE2, guaranteed on x86-64.
    unsafe {
        let zero = _mm_setzero_si128();
        let green_alpha = _mm_set1_epi32(0xff00ff00_u32 as i32);
        let red = _mm_set1_epi32(0x000000ff);
        let blue = _mm_set1_epi32(0x00ff0000);
        let alpha_lanes = _mm_set_epi16(255, 0, 0, 0, 255, 0, 0, 0);
        let rounding = _mm_set1_epi16(128);
        let multiply = |channels| {
            // Two RGBA pixels, unpacked into eight u16 lanes. Preserve their alpha
            // channels by multiplying those lanes by 255, not by alpha again.
            let weights = _mm_or_si128(
                _mm_shufflehi_epi16::<0xff>(_mm_shufflelo_epi16::<0xff>(channels)),
                alpha_lanes,
            );
            let product = _mm_mullo_epi16(channels, weights);
            // Exact round-to-nearest division by 255, equivalent to (product+127)/255
            // for every product of two bytes. Intermediate sums fit in u16.
            let biased = _mm_add_epi16(product, rounding);
            _mm_srli_epi16::<8>(_mm_add_epi16(biased, _mm_srli_epi16::<8>(biased)))
        };
        for block in blocks {
            let rgba = _mm_loadu_si128(block.as_ptr().cast());
            let bgra = _mm_or_si128(
                _mm_and_si128(rgba, green_alpha),
                _mm_or_si128(
                    _mm_slli_epi32::<16>(_mm_and_si128(rgba, red)),
                    _mm_srli_epi32::<16>(_mm_and_si128(rgba, blue)),
                ),
            );
            let low = multiply(_mm_unpacklo_epi8(bgra, zero));
            let high = multiply(_mm_unpackhi_epi8(bgra, zero));
            _mm_storeu_si128(block.as_mut_ptr().cast(), _mm_packus_epi16(low, high));
        }
    }
    scalar(tail);
}

fn scalar(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let [red, green, blue, alpha] = *pixel;
        *pixel = match alpha {
            0 => [0; 4],
            255 => [blue, green, red, 255],
            alpha => {
                let multiply =
                    |channel: u8| ((u16::from(channel) * u16::from(alpha) + 127) / 255) as u8;
                [multiply(blue), multiply(green), multiply(red), alpha]
            }
        };
    }
}

#[cfg(test)]
#[path = "premultiply/tests.rs"]
mod tests;
