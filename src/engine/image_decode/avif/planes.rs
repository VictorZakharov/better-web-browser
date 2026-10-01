//! Copy codec-owned padded planes into bounded, tightly packed Rust arrays.

use super::super::super::{DecodeLimits, DecodeResult};
use super::super::decoder::Picture;
use rav1d::include::dav1d::headers::*;

pub(super) struct Planes<T> {
    pub width: u32,
    pub height: u32,
    pub chroma_width: u32,
    pub y: Vec<T>,
    pub u: Vec<T>,
    pub v: Vec<T>,
}

impl<T: Copy + Default> Planes<T> {
    pub fn copy(picture: &Picture, limits: DecodeLimits) -> DecodeResult<Self> {
        let width = u32::try_from(picture.0.p.w).map_err(|_| "invalid AV1 width")?;
        let height = u32::try_from(picture.0.p.h).map_err(|_| "invalid AV1 height")?;
        limits.rgba_len(width, height)?;
        let (chroma_width, chroma_height) = match picture.0.p.layout {
            DAV1D_PIXEL_LAYOUT_I400 => (0, 0),
            DAV1D_PIXEL_LAYOUT_I420 => (width.div_ceil(2), height.div_ceil(2)),
            DAV1D_PIXEL_LAYOUT_I422 => (width.div_ceil(2), height),
            DAV1D_PIXEL_LAYOUT_I444 => (width, height),
            _ => return Err("unsupported AV1 chroma layout".into()),
        };
        Ok(Self {
            width,
            height,
            chroma_width,
            y: copy_plane(picture, 0, width, height, limits)?,
            u: copy_plane(picture, 1, chroma_width, chroma_height, limits)?,
            v: copy_plane(picture, 2, chroma_width, chroma_height, limits)?,
        })
    }
}

fn copy_plane<T: Copy + Default>(
    picture: &Picture,
    plane: usize,
    width: u32,
    height: u32,
    limits: DecodeLimits,
) -> DecodeResult<Vec<T>> {
    if width == 0 || height == 0 {
        return Ok(Vec::new());
    }
    let bytes_per_sample = std::mem::size_of::<T>();
    if !matches!((picture.0.p.bpc, bytes_per_sample), (8, 1) | (10 | 12, 2)) {
        return Err("AV1 plane sample type does not match its bit depth".into());
    }
    let pointer = picture.0.data[plane]
        .ok_or("AV1 decoder returned a missing plane")?
        .cast::<T>();
    let stride = usize::try_from(picture.0.stride[usize::from(plane != 0)])
        .map_err(|_| "AV1 decoder returned a negative stride")?;
    let row_bytes = width as usize * bytes_per_sample;
    let span = stride
        .checked_mul(height as usize - 1)
        .and_then(|size| size.checked_add(row_bytes))
        .ok_or("AV1 plane span overflow")?;
    if stride < row_bytes
        || !stride.is_multiple_of(bytes_per_sample)
        || !pointer
            .as_ptr()
            .addr()
            .is_multiple_of(std::mem::align_of::<T>())
        || span > limits.working_bytes
    {
        return Err("AV1 plane has invalid or excessive stride".into());
    }
    let count = width as usize * height as usize;
    let mut output = vec![T::default(); count];
    for (row, target) in output.chunks_exact_mut(width as usize).enumerate() {
        // SAFETY: a live rav1d Picture owns its plane; its default allocator
        // guarantees p.w samples per row and p.h rows at the byte stride. The
        // checks above establish alignment, type, arithmetic and budget bounds.
        // Only initialized row samples are read, never decoder padding.
        let source = unsafe {
            std::slice::from_raw_parts(
                pointer.as_ptr().add(row * stride / bytes_per_sample),
                width as usize,
            )
        };
        target.copy_from_slice(source);
    }
    Ok(output)
}
