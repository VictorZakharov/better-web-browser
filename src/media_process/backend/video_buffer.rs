//! Read the native NV12 surface pitch; visible width is not a row-allocation contract.
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer2, IMFMediaType, IMFSample, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE,
    MF2DBuffer_LockFlags_Read, MFGetStrideForBitmapInfoHeader, MFVideoFormat_NV12,
};
use windows::core::Interface;

pub(super) fn default_stride(format: &IMFMediaType, fallback: u32) -> u32 {
    // https://learn.microsoft.com/windows/win32/medfound/mf-mt-default-stride-attribute
    unsafe { format.GetUINT32(&MF_MT_DEFAULT_STRIDE) }.unwrap_or_else(|_| {
        let width = unsafe { format.GetUINT64(&MF_MT_FRAME_SIZE) }
            .map(|size| (size >> 32) as u32)
            .unwrap_or(fallback);
        unsafe { MFGetStrideForBitmapInfoHeader(MFVideoFormat_NV12.data1, width) }
            .ok()
            .and_then(|stride| u32::try_from(stride).ok())
            .unwrap_or(fallback)
    })
}

pub(super) fn copy(
    sample: &IMFSample,
    fallback_stride: u32,
    maximum: u64,
) -> Result<(Vec<u8>, u32), String> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }
        .map_err(|error| format!("get NV12 output buffer: {error}"))?;
    let Ok(surface) = buffer.cast::<IMF2DBuffer2>() else {
        return super::stream::copy_sample(sample, "NV12", maximum)
            .map(|bytes| (bytes, fallback_stride));
    };
    let mut scanline = std::ptr::null_mut();
    let mut start = std::ptr::null_mut();
    let mut pitch = 0;
    let mut length = 0;
    unsafe {
        surface.Lock2DSize(
            MF2DBuffer_LockFlags_Read,
            &mut scanline,
            &mut pitch,
            &mut start,
            &mut length,
        )
    }
    .map_err(|error| format!("lock NV12 surface: {error}"))?;
    let result = if pitch <= 0
        || start.is_null()
        || scanline != start
        || length == 0
        || u64::from(length) > maximum
    {
        Err("NV12 surface has an invalid top-down allocation".into())
    } else {
        // Lock2DSize supplies the accessible allocation length, not an inferred pointer extent.
        Ok((
            unsafe { std::slice::from_raw_parts(start, length as usize) }.to_vec(),
            pitch as u32,
        ))
    };
    let unlocked =
        unsafe { surface.Unlock2D() }.map_err(|error| format!("unlock NV12 surface: {error}"));
    match (result, unlocked) {
        (Ok(frame), Ok(())) => Ok(frame),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::MediaFoundation::{MFCreate2DMediaBuffer, MFCreateSample};

    #[test]
    fn padded_native_nv12_rows_preserve_the_actual_surface_pitch() {
        let _apartment = crate::media_process::backend::ComApartment::initialize().unwrap();
        let _foundation = crate::media_process::backend::MediaFoundation::start().unwrap();
        let buffer =
            unsafe { MFCreate2DMediaBuffer(854, 480, MFVideoFormat_NV12.data1, false) }.unwrap();
        let sample = unsafe { MFCreateSample() }.unwrap();
        unsafe { sample.AddBuffer(&buffer) }.unwrap();
        let (bytes, stride) = copy(&sample, 854, 2 * 1024 * 1024).unwrap();
        assert!(stride >= 854);
        assert_eq!(bytes.len() as u64 * 2, u64::from(stride) * 480 * 3);
    }
}
