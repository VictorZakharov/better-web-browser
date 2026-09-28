//! Device enumeration and Source Reader configuration. This module is called only inside the
//! capability-bearing capture child, after the browser's one-shot grant handshake.

use super::{classify_error, samples};
use crate::capture_protocol::{CaptureFailure, CaptureSample, CaptureSampleKind};
use std::collections::VecDeque;
use std::ptr::null_mut;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaSource, IMFSourceReader,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MFAudioFormat_PCM,
    MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromMediaSource,
    MFEnumDeviceSources, MFMediaType_Audio, MFMediaType_Video, MFVideoFormat_NV12,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::GUID;

const VIDEO_TRACK: u64 = 1;
const AUDIO_TRACK: u64 = 2;
const MAX_VIDEO_WIDTH: u32 = 1280;
const MAX_VIDEO_HEIGHT: u32 = 720;

struct DeviceSource {
    // The reader is released before the source is shut down in Drop.
    reader: Option<IMFSourceReader>,
    source: IMFMediaSource,
    stream: u32,
}

impl DeviceSource {
    fn open(kind: GUID, stream: u32, advanced_video: bool) -> Result<Self, CaptureFailure> {
        let device_attributes = attributes()?;
        unsafe { device_attributes.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &kind) }
            .map_err(classify_error)?;
        let source = first_device(&device_attributes)?;
        let reader_attributes = attributes()?;
        if advanced_video {
            unsafe {
                reader_attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
            }
            .map_err(classify_error)?;
        }
        let reader =
            match unsafe { MFCreateSourceReaderFromMediaSource(&source, &reader_attributes) } {
                Ok(reader) => reader,
                Err(error) => {
                    // An activated device source needs explicit Shutdown even when reader creation
                    // fails; dropping the COM pointer alone can leave hardware resources alive.
                    let _ = unsafe { source.Shutdown() };
                    return Err(classify_error(error));
                }
            };
        Ok(Self {
            reader: Some(reader),
            source,
            stream,
        })
    }

    fn reader(&self) -> &IMFSourceReader {
        self.reader.as_ref().expect("source reader is live")
    }
}

impl Drop for DeviceSource {
    fn drop(&mut self) {
        self.reader.take();
        let _ = unsafe { self.source.Shutdown() };
    }
}

pub(super) struct CameraCapture {
    device: DeviceSource,
    width: u32,
    height: u32,
    stride: u32,
    sequence: u64,
}

impl CameraCapture {
    pub(super) fn open() -> Result<Self, CaptureFailure> {
        let device = DeviceSource::open(
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
            true,
        )?;
        let reader = device.reader();
        let mut selected = None;
        // A low-resolution native camera mode is selected before Source Reader conversion.
        // There is no silent upscaling of a >720p mode into a supposedly bounded live sample.
        for index in 0..64 {
            let Ok(native) = (unsafe { reader.GetNativeMediaType(device.stream, index) }) else {
                break;
            };
            let Ok(size) = (unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }) else {
                continue;
            };
            let (width, height) = ((size >> 32) as u32, size as u32);
            if width == 0
                || height == 0
                || width > MAX_VIDEO_WIDTH
                || height > MAX_VIDEO_HEIGHT
                || !width.is_multiple_of(2)
                || !height.is_multiple_of(2)
            {
                continue;
            }
            if selected
                .as_ref()
                .is_none_or(|(_, pixels)| u64::from(width) * u64::from(height) > *pixels)
            {
                selected = Some((native, u64::from(width) * u64::from(height)));
            }
        }
        let (native, _) = selected.ok_or(CaptureFailure::InvalidFormat)?;
        let size = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(classify_error)?;
        let (width, height) = ((size >> 32) as u32, size as u32);
        unsafe { reader.SetCurrentMediaType(device.stream, None, &native) }
            .map_err(classify_error)?;
        let output = unsafe { MFCreateMediaType() }.map_err(classify_error)?;
        unsafe { output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }.map_err(classify_error)?;
        unsafe { output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12) }.map_err(classify_error)?;
        unsafe { output.SetUINT64(&MF_MT_FRAME_SIZE, size) }.map_err(classify_error)?;
        unsafe { reader.SetCurrentMediaType(device.stream, None, &output) }
            .map_err(|_| CaptureFailure::InvalidFormat)?;
        let current =
            unsafe { reader.GetCurrentMediaType(device.stream) }.map_err(classify_error)?;
        let actual_size =
            unsafe { current.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(classify_error)?;
        let subtype = unsafe { current.GetGUID(&MF_MT_SUBTYPE) }.map_err(classify_error)?;
        if actual_size != size || subtype != MFVideoFormat_NV12 {
            return Err(CaptureFailure::InvalidFormat);
        }
        let stride = unsafe { current.GetUINT32(&MF_MT_DEFAULT_STRIDE) }.unwrap_or(width);
        Ok(Self {
            device,
            width,
            height,
            stride,
            sequence: 0,
        })
    }

    pub(super) fn next_sample(
        &mut self,
        capture_id: u64,
    ) -> Result<Option<CaptureSample>, CaptureFailure> {
        let Some((sample, timestamp)) = samples::read(self.device.reader(), self.device.stream)?
        else {
            return Ok(None);
        };
        let (bytes, stride) = samples::copy_nv12(&sample, self.stride)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(CaptureFailure::Internal)?;
        let frame = CaptureSample {
            capture_id,
            track_id: VIDEO_TRACK,
            sequence: self.sequence,
            timestamp_100ns: timestamp.max(0) as u64,
            kind: CaptureSampleKind::VideoNv12,
            width_or_rate: self.width,
            height_or_frames: self.height,
            stride_or_channels: stride,
            bytes,
        };
        frame
            .validate()
            .map_err(|_| CaptureFailure::InvalidFormat)?;
        Ok(Some(frame))
    }
}

pub(super) struct MicrophoneCapture {
    device: DeviceSource,
    rate: u32,
    channels: u32,
    sequence: u64,
    pending: VecDeque<CaptureSample>,
}

impl MicrophoneCapture {
    pub(super) fn open() -> Result<Self, CaptureFailure> {
        let device = DeviceSource::open(
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID,
            MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
            false,
        )?;
        let reader = device.reader();
        let mut chosen = None;
        for (rate, channels) in [
            (48_000, 1),
            (48_000, 2),
            (44_100, 1),
            (44_100, 2),
            (16_000, 1),
        ] {
            let output = unsafe { MFCreateMediaType() }.map_err(classify_error)?;
            unsafe { output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio) }
                .map_err(classify_error)?;
            unsafe { output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM) }
                .map_err(classify_error)?;
            unsafe { output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16) }
                .map_err(classify_error)?;
            unsafe { output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate) }
                .map_err(classify_error)?;
            unsafe { output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels) }
                .map_err(classify_error)?;
            unsafe { output.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2) }
                .map_err(classify_error)?;
            unsafe { output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * channels * 2) }
                .map_err(classify_error)?;
            if unsafe { reader.SetCurrentMediaType(device.stream, None, &output) }.is_ok() {
                chosen = Some((rate, channels));
                break;
            }
        }
        let (rate, channels) = chosen.ok_or(CaptureFailure::InvalidFormat)?;
        let current =
            unsafe { reader.GetCurrentMediaType(device.stream) }.map_err(classify_error)?;
        if unsafe { current.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE) }.ok() != Some(16)
            || unsafe { current.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.ok() != Some(rate)
            || unsafe { current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.ok() != Some(channels)
            || unsafe { current.GetGUID(&MF_MT_SUBTYPE) }.ok() != Some(MFAudioFormat_PCM)
        {
            return Err(CaptureFailure::InvalidFormat);
        }
        Ok(Self {
            device,
            rate,
            channels,
            sequence: 0,
            pending: VecDeque::new(),
        })
    }

    pub(super) fn next_sample(
        &mut self,
        capture_id: u64,
    ) -> Result<Option<CaptureSample>, CaptureFailure> {
        if let Some(sample) = self.pending.pop_front() {
            return Ok(Some(sample));
        }
        let Some((sample, timestamp)) = samples::read(self.device.reader(), self.device.stream)?
        else {
            return Ok(None);
        };
        let bytes = samples::copy_pcm(&sample)?;
        let alignment = (self.channels * 2) as usize;
        if bytes.is_empty() || !bytes.len().is_multiple_of(alignment) {
            return Err(CaptureFailure::InvalidFormat);
        }
        let frames_per_packet = (self.rate / 50).min(960) as usize; // at most 20 ms.
        let mut frame_offset = 0_usize;
        for chunk in bytes.chunks(frames_per_packet * alignment) {
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(CaptureFailure::Internal)?;
            let packet = CaptureSample {
                capture_id,
                track_id: AUDIO_TRACK,
                sequence: self.sequence,
                timestamp_100ns: (timestamp.max(0) as u64)
                    .saturating_add(frame_offset as u64 * 10_000_000 / u64::from(self.rate)),
                kind: CaptureSampleKind::AudioPcm16,
                width_or_rate: self.rate,
                height_or_frames: (chunk.len() / alignment) as u32,
                stride_or_channels: self.channels,
                bytes: chunk.to_vec(),
            };
            packet
                .validate()
                .map_err(|_| CaptureFailure::InvalidFormat)?;
            self.pending.push_back(packet);
            frame_offset += chunk.len() / alignment;
        }
        Ok(self.pending.pop_front())
    }
}

fn attributes() -> Result<IMFAttributes, CaptureFailure> {
    let mut attributes = None;
    unsafe { MFCreateAttributes(&mut attributes, 4) }.map_err(classify_error)?;
    attributes.ok_or(CaptureFailure::Internal)
}

fn first_device(attributes: &IMFAttributes) -> Result<IMFMediaSource, CaptureFailure> {
    let mut pointer: *mut Option<IMFActivate> = null_mut();
    let mut count = 0_u32;
    let result = unsafe { MFEnumDeviceSources(attributes, &mut pointer, &mut count) };
    let activations = ActivationList { pointer, count };
    result.map_err(classify_error)?;
    if count == 0 || pointer.is_null() {
        return Err(CaptureFailure::NoDevice);
    }
    let activation = unsafe { &mut *pointer }
        .take()
        .ok_or(CaptureFailure::NoDevice)?;
    let source =
        unsafe { activation.ActivateObject::<IMFMediaSource>() }.map_err(classify_error)?;
    drop(activation);
    drop(activations);
    Ok(source)
}

struct ActivationList {
    pointer: *mut Option<IMFActivate>,
    count: u32,
}

impl Drop for ActivationList {
    fn drop(&mut self) {
        if self.pointer.is_null() {
            return;
        }
        let activations =
            unsafe { std::slice::from_raw_parts_mut(self.pointer, self.count as usize) };
        for activation in activations {
            drop(activation.take());
        }
        unsafe { CoTaskMemFree(Some(self.pointer.cast())) };
    }
}
