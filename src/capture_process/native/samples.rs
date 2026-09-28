//! Bounded copies from Media Foundation-owned buffers into the private capture protocol.

use super::{CaptureFailure, classify_error};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer2, IMFSample, IMFSourceReader, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
    MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED, MF2DBuffer_LockFlags_Read,
};
use windows::core::Interface;

const MAX_FRAME_BYTES: u64 = 2 * 1024 * 1024;
const MAX_AUDIO_SOURCE_BYTES: u64 = 256 * 1024;

pub(super) fn read(
    reader: &IMFSourceReader,
    stream: u32,
) -> Result<Option<(IMFSample, i64)>, CaptureFailure> {
    let mut flags = 0_u32;
    let mut timestamp = 0_i64;
    let mut sample = None;
    unsafe {
        reader.ReadSample(
            stream,
            0,
            None,
            Some(&mut flags),
            Some(&mut timestamp),
            Some(&mut sample),
        )
    }
    .map_err(classify_error)?;
    if flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
        return Err(CaptureFailure::Internal);
    }
    if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
        return Err(CaptureFailure::NoDevice);
    }
    if flags
        & ((MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0
            | MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED.0) as u32)
        != 0
    {
        // A packet cannot be interpreted using the originally granted output format.
        return Err(CaptureFailure::InvalidFormat);
    }
    Ok(sample.map(|sample| (sample, timestamp)))
}

pub(super) fn copy_nv12(
    sample: &IMFSample,
    fallback_stride: u32,
) -> Result<(Vec<u8>, u32), CaptureFailure> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }.map_err(classify_error)?;
    let Ok(surface) = buffer.cast::<IMF2DBuffer2>() else {
        return copy_linear(sample, MAX_FRAME_BYTES).map(|bytes| (bytes, fallback_stride));
    };
    let mut scanline = std::ptr::null_mut();
    let mut start = std::ptr::null_mut();
    let mut pitch = 0_i32;
    let mut length = 0_u32;
    unsafe {
        surface.Lock2DSize(
            MF2DBuffer_LockFlags_Read,
            &mut scanline,
            &mut pitch,
            &mut start,
            &mut length,
        )
    }
    .map_err(classify_error)?;
    let copied = if pitch <= 0
        || start.is_null()
        || scanline != start
        || length == 0
        || u64::from(length) > MAX_FRAME_BYTES
    {
        Err(CaptureFailure::InvalidFormat)
    } else {
        Ok((
            unsafe { std::slice::from_raw_parts(start, length as usize) }.to_vec(),
            pitch as u32,
        ))
    };
    let unlocked = unsafe { surface.Unlock2D() }.map_err(classify_error);
    match (copied, unlocked) {
        (Ok(frame), Ok(())) => Ok(frame),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

pub(super) fn copy_pcm(sample: &IMFSample) -> Result<Vec<u8>, CaptureFailure> {
    copy_linear(sample, MAX_AUDIO_SOURCE_BYTES)
}

fn copy_linear(sample: &IMFSample, maximum: u64) -> Result<Vec<u8>, CaptureFailure> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }.map_err(classify_error)?;
    let mut pointer = std::ptr::null_mut();
    let mut allocation = 0_u32;
    let mut length = 0_u32;
    unsafe { buffer.Lock(&mut pointer, Some(&mut allocation), Some(&mut length)) }
        .map_err(classify_error)?;
    let copied =
        if length > allocation || u64::from(length) > maximum || (length != 0 && pointer.is_null())
        {
            Err(CaptureFailure::InvalidFormat)
        } else if length == 0 {
            Ok(Vec::new())
        } else {
            Ok(unsafe { std::slice::from_raw_parts(pointer, length as usize) }.to_vec())
        };
    let unlocked = unsafe { buffer.Unlock() }.map_err(classify_error);
    match (copied, unlocked) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}
