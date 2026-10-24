use super::{rgba_to_bgra, scalar};

#[test]
fn every_channel_alpha_pair_matches_the_scalar_contract() {
    let mut pixels = Vec::with_capacity(256 * 256 * 4);
    for alpha in 0..=255_u8 {
        for channel in 0..=255_u8 {
            pixels.extend_from_slice(&[channel, 255 - channel, channel.rotate_left(3), alpha]);
        }
    }
    let mut expected = pixels.clone();
    scalar(&mut expected);
    rgba_to_bgra(&mut pixels);
    assert_eq!(pixels, expected);
}

#[test]
fn arbitrary_alignment_and_short_tails_never_touch_neighboring_bytes() {
    for offset in 0..32 {
        for length in 0..96 {
            let mut actual: Vec<u8> = (0..offset + length + 32)
                .map(|i| (i * 71 + 23) as u8)
                .collect();
            let mut expected = actual.clone();
            scalar(&mut expected[offset..offset + length]);
            rgba_to_bgra(&mut actual[offset..offset + length]);
            assert_eq!(actual, expected, "offset={offset}, length={length}");
        }
    }
}

#[test]
fn transparent_hidden_rgb_is_zeroed_and_opaque_colors_are_only_swizzled() {
    let mut pixels = [
        11, 22, 33, 0, 44, 55, 66, 255, 77, 88, 99, 0, 101, 102, 103, 255,
    ];
    rgba_to_bgra(&mut pixels);
    assert_eq!(
        pixels,
        [0, 0, 0, 0, 66, 55, 44, 255, 0, 0, 0, 0, 103, 102, 101, 255]
    );
}
