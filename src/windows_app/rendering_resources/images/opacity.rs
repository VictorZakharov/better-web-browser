//! Classify immutable, validated BGRA bytes once per bitmap admission/update.
//! Transparency is never inferred from a URL, canvas attribute or author declaration.
pub(super) fn all_opaque(bytes: &[u8]) -> bool {
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return false;
    }
    #[cfg(target_arch = "x86_64")]
    {
        // SSE2 is mandatory on x86-64. Unaligned loads stay inside each exact chunk.
        use std::arch::x86_64::*;
        let mut chunks = bytes.chunks_exact(16);
        unsafe {
            let full = _mm_set1_epi8(-1);
            for chunk in &mut chunks {
                let pixels = _mm_loadu_si128(chunk.as_ptr().cast());
                if _mm_movemask_epi8(_mm_cmpeq_epi8(pixels, full)) & 0x8888 != 0x8888 {
                    return false;
                }
            }
        }
        chunks
            .remainder()
            .chunks_exact(4)
            .all(|pixel| pixel[3] == 255)
    }
    #[cfg(not(target_arch = "x86_64"))]
    bytes.chunks_exact(4).all(|pixel| pixel[3] == 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifier_checks_every_alpha_lane_and_scalar_tail_without_inspecting_rgb() {
        for count in 1..18 {
            let mut pixels = [12, 255, 0, 255].repeat(count);
            assert!(all_opaque(&pixels));
            for index in 0..count {
                for alpha in 0..255 {
                    pixels[index * 4 + 3] = alpha;
                    assert!(
                        !all_opaque(&pixels),
                        "count={count}, index={index}, alpha={alpha}"
                    );
                }
                pixels[index * 4 + 3] = 255;
            }
        }
        assert!(!all_opaque(&[]));
        assert!(!all_opaque(&[0, 0, 0, 255, 0]));
    }
}
