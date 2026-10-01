//! Project-owned BMFF metadata construction, independent of the parser under test.
use crate::engine::image_decode::{DecodeLimits, RasterImage};

pub(super) fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut result = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    result.extend_from_slice(kind);
    result.extend_from_slice(payload);
    result
}

pub(super) fn concatenate(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

pub(super) fn full_box(version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut output = flags.to_be_bytes().to_vec();
    output[0] = version;
    output.extend_from_slice(payload);
    output
}

pub(super) fn associations(version: u8, wide: bool, entries: &[(u32, Vec<u16>)]) -> Vec<u8> {
    let mut payload = (entries.len() as u32).to_be_bytes().to_vec();
    for (item, properties) in entries {
        if version == 0 {
            payload.extend_from_slice(&(*item as u16).to_be_bytes());
        } else {
            payload.extend_from_slice(&item.to_be_bytes());
        }
        payload.push(properties.len() as u8);
        for &property in properties {
            if wide {
                payload.extend_from_slice(&property.to_be_bytes());
            } else {
                payload.push(property as u8);
            }
        }
    }
    boxed(b"ipma", &full_box(version, u32::from(wide), &payload))
}

pub(super) fn metadata(primary: u32, properties: &[Vec<u8>], associations: &[Vec<u8>]) -> Vec<u8> {
    let version = u8::from(primary > u16::MAX as u32);
    let id = if version == 0 {
        (primary as u16).to_be_bytes().to_vec()
    } else {
        primary.to_be_bytes().to_vec()
    };
    let pitm = boxed(b"pitm", &full_box(version, 0, &id));
    let mut iprp = boxed(b"ipco", &concatenate(properties));
    iprp.extend(concatenate(associations));
    boxed(
        b"meta",
        &full_box(0, 0, &concatenate(&[pitm, boxed(b"iprp", &iprp)])),
    )
}

pub(super) fn image() -> RasterImage {
    RasterImage::new(
        3,
        2,
        [1, 2, 3, 4, 5, 6]
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect(),
        DecodeLimits::CANVAS,
    )
    .unwrap()
}

pub(super) fn red_pixels(image: &RasterImage) -> Vec<u8> {
    image.rgba.chunks_exact(4).map(|pixel| pixel[0]).collect()
}

pub(super) fn spatial(width: u32, height: u32) -> Vec<u8> {
    let mut payload = width.to_be_bytes().to_vec();
    payload.extend_from_slice(&height.to_be_bytes());
    boxed(b"ispe", &full_box(0, 0, &payload))
}

pub(super) fn aperture(
    width: (u32, u32),
    height: (u32, u32),
    x: (i32, u32),
    y: (i32, u32),
) -> Vec<u8> {
    let values = [
        width.0, width.1, height.0, height.1, x.0 as u32, x.1, y.0 as u32, y.1,
    ];
    values.into_iter().flat_map(u32::to_be_bytes).collect()
}
