//! Shared RGBA8 associated-alpha recovery for codecs and GPU bitmap consumers.
//! This is byte arithmetic, not color-space conversion or a drawing operation.
use std::sync::OnceLock;

const COMPONENTS: usize = 256 * 256;
static STRAIGHT: OnceLock<Box<[u8]>> = OnceLock::new();

pub(crate) fn unpremultiply_rgba(pixels: &mut [u8]) {
    // One immutable 64 KiB table per process. Allocate it on the heap rather
    // than the bounded renderer/worker stack; never cache author buffers.
    let straight = STRAIGHT.get_or_init(|| {
        let mut table = vec![0; COMPONENTS];
        for alpha in 1..256_u32 {
            for channel in 0..256_u32 {
                table[(alpha * 256 + channel) as usize] =
                    ((channel * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
        table.into_boxed_slice()
    });
    for pixel in pixels.chunks_exact_mut(4) {
        let row = usize::from(pixel[3]) * 256;
        if pixel[3] == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = straight[row + usize::from(*channel)];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_byte_pairs_match_existing_integer_and_javascript_rounding() {
        let mut pixels = Vec::with_capacity(COMPONENTS * 4);
        for alpha in 0..=255_u32 {
            for channel in 0..=255_u32 {
                pixels.extend_from_slice(&[channel as u8, (255 - channel) as u8, 173, alpha as u8]);
            }
        }
        let original = pixels.clone();
        unpremultiply_rgba(&mut pixels);
        for (actual, input) in pixels.chunks_exact(4).zip(original.chunks_exact(4)) {
            let alpha = u32::from(input[3]);
            assert_eq!(actual[3], input[3]);
            for index in 0..3 {
                let integer = (u32::from(input[index]) * 255 + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8;
                let js = if alpha == 0 {
                    0
                } else {
                    (f64::from(input[index]) * 255. / f64::from(alpha))
                        .round()
                        .min(255.) as u8
                };
                assert_eq!(actual[index], integer, "integer {input:?}");
                assert_eq!(actual[index], js, "JS {input:?}");
            }
        }
    }

    #[test]
    fn recovery_touches_only_owned_complete_pixels_and_normalizes_hidden_rgb() {
        let source = [33, 77, 99, 0, 3, 7, 11, 255, 16, 32, 64, 64, 17, 19, 23];
        let mut first = source;
        let second = source;
        unpremultiply_rgba(&mut first);
        assert_eq!(
            first,
            [0, 0, 0, 0, 3, 7, 11, 255, 64, 128, 255, 64, 17, 19, 23]
        );
        assert_eq!(second, source);
        unpremultiply_rgba(&mut []);
        for count in 0..4 {
            let mut tail = vec![117; count];
            unpremultiply_rgba(&mut tail);
            assert_eq!(tail, vec![117; count]);
        }
    }
}
