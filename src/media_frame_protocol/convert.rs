use super::{MediaFrameError, MediaVideoFrameMetadata};
use crate::engine::DecodedImage;
use crate::limits::MAX_MEDIA_DECODED_FRAME_BYTES;

pub(crate) fn nv12_to_bgra(
    metadata: MediaVideoFrameMetadata,
    nv12: &[u8],
) -> Result<DecodedImage, MediaFrameError> {
    metadata.validate()?;
    if nv12.len() as u64 != metadata.data_length {
        return Err(MediaFrameError::InvalidLength(nv12.len() as u64));
    }
    let pixel_count = u64::from(metadata.width)
        .checked_mul(u64::from(metadata.height))
        .ok_or(MediaFrameError::InvalidDimensions)?;
    let bgra_length = pixel_count
        .checked_mul(4)
        .ok_or(MediaFrameError::InvalidLength(metadata.data_length))?;
    if bgra_length > MAX_MEDIA_DECODED_FRAME_BYTES as u64 {
        return Err(MediaFrameError::InvalidLength(bgra_length));
    }
    let mut bgra = vec![0_u8; bgra_length as usize];
    let stride = metadata.stride as usize;
    let chroma_start = stride * metadata.height as usize;
    for y in 0..metadata.height as usize {
        for x in 0..metadata.width as usize {
            let luma = i32::from(nv12[y * stride + x]);
            let chroma = chroma_start + (y / 2) * stride + (x & !1);
            let u = i32::from(nv12[chroma]);
            let v = i32::from(nv12[chroma + 1]);
            let c = (luma - 16).max(0);
            let d = u - 128;
            let e = v - 128;
            let red = clamp((298 * c + 409 * e + 128) >> 8);
            let green = clamp((298 * c - 100 * d - 208 * e + 128) >> 8);
            let blue = clamp((298 * c + 516 * d + 128) >> 8);
            let output = (y * metadata.width as usize + x) * 4;
            bgra[output..output + 4].copy_from_slice(&[blue, green, red, 255]);
        }
    }
    Ok(DecodedImage {
        width: metadata.width,
        height: metadata.height,
        bgra,
    })
}

fn clamp(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}
