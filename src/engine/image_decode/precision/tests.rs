use super::*;
use crate::engine::image_decode::{DecodeOptions, decode, decode_precise};
use image::{ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;

fn png(words: Vec<u16>, width: u32, height: u32) -> Vec<u8> {
    let image = ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, words).unwrap();
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).unwrap();
    output.into_inner()
}

#[test]
fn precise_decode_preserves_every_source_word_and_does_not_invent_page_precision() {
    let words = [1, 32769, 65534, 257, 65, 129, 193, 65535].to_vec();
    let bytes = png(words.clone(), 2, 1);
    let precise = decode_precise(&bytes, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    assert_eq!(precise.rgba16.as_ref().unwrap(), &words);
    assert_eq!(
        precise.rgba,
        words.iter().copied().map(narrow).collect::<Vec<_>>()
    );
    let ordinary = decode(&bytes, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    assert!(ordinary.rgba16.is_none());
    assert_eq!(ordinary.rgba, precise.rgba);
}

#[test]
fn eight_bit_sources_do_not_grow_a_synthetic_precision_sidecar() {
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(1, 1, vec![1, 2, 3, 255]).unwrap();
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).unwrap();
    let precise = decode_precise(
        &output.into_inner(),
        DecodeLimits::CANVAS,
        Default::default(),
    )
    .unwrap();
    assert!(precise.rgba16.is_none());
    assert_eq!(precise.rgba, [1, 2, 3, 255]);
}

#[test]
fn precise_expansion_accounts_for_decoder_destination_and_cms_row_scratch() {
    let bytes = png(vec![65535; 16], 2, 2);
    let limits = DecodeLimits {
        working_bytes: 103,
        ..DecodeLimits::CANVAS
    };
    assert!(
        decode_precise(&bytes, limits, Default::default())
            .unwrap_err()
            .contains("budget")
    );
    let limits = DecodeLimits {
        working_bytes: 104,
        ..limits
    };
    assert!(decode_precise(&bytes, limits, Default::default()).is_ok());
}

#[test]
fn precise_cms_preserves_alpha_and_refreshes_the_byte_view_from_transformed_words() {
    let bytes = png(vec![32769, 12345, 54321, 257, 1, 65, 129, 65534], 2, 1);
    let mut precise = decode_precise(&bytes, DecodeLimits::CANVAS, Default::default()).unwrap();
    let before = precise.rgba16.clone().unwrap();
    precise
        .apply_icc(&moxcms::ColorProfile::new_srgb().encode().unwrap())
        .unwrap();
    let words = precise.rgba16.as_ref().unwrap();
    assert_eq!(words[3], 257);
    assert_eq!(words[7], 65534);
    for (index, (&actual, &expected)) in words.iter().zip(&before).enumerate() {
        assert!(actual.abs_diff(expected) <= if index % 4 == 3 { 0 } else { 8 });
    }
    assert_eq!(
        precise.rgba,
        words.iter().copied().map(narrow).collect::<Vec<_>>()
    );
}

#[test]
fn normalized_narrowing_is_monotonic_and_rounds_extremes_without_overflow() {
    assert_eq!(narrow(0), 0);
    assert_eq!(narrow(65535), 255);
    for value in 0..65535 {
        assert!(narrow(value) <= narrow(value + 1));
    }
}
