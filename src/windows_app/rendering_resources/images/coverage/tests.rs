use super::*;
fn image(width: u32, height: u32, pixel: [u8; 4]) -> DecodedImage {
    DecodedImage {
        width,
        height,
        bgra: pixel.repeat(width as usize * height as usize).into(),
    }
}
#[test]
fn bounded_cover_is_disjoint_and_cannot_skip_any_nonzero_channel() {
    let mut source = image(427, 770, [0; 4]);
    let mut pixels = source.bgra.to_vec();
    for y in 0..770usize {
        for x in 0..427usize {
            if !(2..425).contains(&x) || !(2..768).contains(&y) {
                pixels[(y * 427 + x) * 4..(y * 427 + x) * 4 + 4]
                    .copy_from_slice(&[12, 24, 48, 128]);
            }
        }
    }
    // A non-premultiplied byte must be retained even though its alpha is zero.
    pixels[(300 * 427 + 200) * 4] = 17;
    source.bgra = pixels.into();
    let coverage = derive(&source).unwrap();
    assert!(coverage.len() <= LIMIT);
    for y in 0..770 {
        for x in 0..427 {
            let count = coverage
                .iter()
                .filter(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                .count();
            assert!(count <= 1);
            if source.bgra
                [(y as usize * 427 + x as usize) * 4..(y as usize * 427 + x as usize) * 4 + 4]
                .iter()
                .any(|b| *b != 0)
            {
                assert_eq!(count, 1);
            }
        }
    }
}
#[test]
fn dense_small_and_excessively_fragmented_images_keep_the_single_draw_path() {
    assert!(derive(&image(256, 256, [0, 0, 0, 1])).is_none());
    assert!(derive(&image(64, 64, [0; 4])).is_none());
    assert!(derive(&image(256, 256, [0; 4])).unwrap().is_empty());
    let mut image = image(2048, 2048, [0; 4]);
    let mut bytes = image.bgra.to_vec();
    for y in (0..2048usize).step_by(128) {
        for x in (0..2048usize).step_by(128) {
            bytes[(y * 2048 + x) * 4] = 1;
        }
    }
    image.bgra = bytes.into();
    assert!(derive(&image).is_none());
}
