use super::fragmented_mp4::{self, VideoSample, VideoTrack};
use super::{ActivationList, ComApartment, MediaFoundation, stream::copy_sample};
use crate::limits::MAX_MEDIA_DURATION_100NS;
use crate::media_process::backend::playback::DecodedVideoSample;
use crate::media_protocol::MediaLimits;
use std::mem::ManuallyDrop;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFMediaType, IMFSample, IMFTransform, MF_E_TRANSFORM_NEED_MORE_INPUT,
    MF_E_TRANSFORM_STREAM_CHANGE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_SUBTYPE, MFCreateMediaType, MFCreateMemoryBuffer,
    MFCreateSample, MFGetStrideForBitmapInfoHeader, MFMediaType_Video,
    MFSampleExtension_CleanPoint, MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_FLAG_SORTANDFILTER_WEB_ONLY,
    MFT_ENUM_FLAG_SYNCMFT, MFT_MESSAGE_COMMAND_DRAIN, MFT_MESSAGE_COMMAND_FLUSH,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_END_OF_STREAM,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
    MFT_REGISTER_TYPE_INFO, MFVideoFormat_H264, MFVideoFormat_NV12,
    MFVideoInterlace_MixedInterlaceOrProgressive,
};

pub(super) struct TransformVideoDecoder {
    transform: IMFTransform,
    samples: Vec<VideoSample>,
    next_input: usize,
    width: u32,
    height: u32,
    stride: u32,
    nal_length_size: usize,
    sequence_header: Vec<u8>,
    header_pending: bool,
    maximum_frame_bytes: u64,
    draining: bool,
    seek_target: Option<u64>,
    frames_emitted: usize,
    _foundation: MediaFoundation,
    _apartment: ComApartment,
}

impl TransformVideoDecoder {
    pub(super) fn open(track: VideoTrack, limits: MediaLimits) -> Result<Self, String> {
        let apartment = ComApartment::initialize()
            .map_err(|status| format!("initialize H.264 COM apartment: HRESULT {status:#x}"))?;
        let foundation = MediaFoundation::start()
            .map_err(|status| format!("start H.264 Media Foundation: HRESULT {status:#x}"))?;
        let transform = activate()?;
        let input = input_type(&track)?;
        unsafe { transform.SetInputType(0, &input, 0) }
            .map_err(|error| format!("configure H.264 transform input: {error}"))?;
        set_nv12_output(&transform)?;
        unsafe {
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .and_then(|_| transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0))
        }
        .map_err(|error| format!("start H.264 transform stream: {error}"))?;
        let stride =
            unsafe { MFGetStrideForBitmapInfoHeader(MFVideoFormat_NV12.data1, track.width) }
                .map_err(|error| format!("read H.264 NV12 stride: {error}"))?;
        let stride = u32::try_from(stride).map_err(|_| "H.264 NV12 stride is negative")?;
        Ok(Self {
            transform,
            samples: track.samples,
            next_input: 0,
            width: track.width,
            height: track.height,
            stride,
            nal_length_size: track.nal_length_size,
            sequence_header: track.sequence_header,
            header_pending: true,
            maximum_frame_bytes: limits.max_decoded_frame_bytes,
            draining: false,
            seek_target: None,
            frames_emitted: 0,
            _foundation: foundation,
            _apartment: apartment,
        })
    }

    pub(super) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(super) fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        let mut index = self
            .samples
            .partition_point(|sample| sample.timestamp_100ns.max(0) as u64 <= position_100ns);
        index = index.saturating_sub(1);
        while index > 0 && !self.samples[index].key_frame {
            index -= 1;
        }
        unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
                .and_then(|_| {
                    self.transform
                        .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                })
        }
        .map_err(|error| format!("seek H.264 transform: {error}"))?;
        self.next_input = index;
        self.draining = false;
        self.header_pending = true;
        self.seek_target = Some(position_100ns);
        Ok(())
    }

    pub(super) fn next_frame(&mut self) -> Result<Option<DecodedVideoSample>, String> {
        loop {
            match self.pull()? {
                Pull::Frame(frame) => {
                    if self.seek_target.is_some_and(|target| {
                        frame.timestamp_100ns.max(0) as u64 + frame.duration_100ns < target
                    }) {
                        continue;
                    }
                    self.seek_target = None;
                    self.frames_emitted += 1;
                    return Ok(Some(frame));
                }
                Pull::NeedInput if self.next_input < self.samples.len() => self.push_next()?,
                Pull::NeedInput if !self.draining => {
                    unsafe {
                        self.transform
                            .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)
                            .and_then(|_| {
                                self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                            })
                    }
                    .map_err(|error| format!("drain H.264 transform: {error}"))?;
                    self.draining = true;
                }
                Pull::NeedInput if self.frames_emitted == 0 => {
                    return Err(format!(
                        "H.264 transform accepted {} access units but emitted no frame ({})",
                        self.next_input,
                        access_unit_summary(&self.samples[0].bytes, self.nal_length_size)
                    ));
                }
                Pull::NeedInput => return Ok(None),
            }
        }
    }

    fn push_next(&mut self) -> Result<(), String> {
        let source = &self.samples[self.next_input];
        let access_unit = fragmented_mp4::annex_b_sample(&source.bytes, self.nal_length_size)?;
        let bytes = if self.header_pending {
            let mut bytes = Vec::with_capacity(self.sequence_header.len() + access_unit.len());
            bytes.extend_from_slice(&self.sequence_header);
            bytes.extend_from_slice(&access_unit);
            self.header_pending = false;
            bytes
        } else {
            access_unit
        };
        let sample = media_sample(
            &bytes,
            source.timestamp_100ns,
            source.duration_100ns,
            source.key_frame,
        )?;
        unsafe { self.transform.ProcessInput(0, &sample, 0) }
            .map_err(|error| format!("submit H.264 access unit: {error}"))?;
        self.next_input += 1;
        Ok(())
    }

    fn pull(&self) -> Result<Pull, String> {
        let info = unsafe { self.transform.GetOutputStreamInfo(0) }
            .map_err(|error| format!("read H.264 output stream info: {error}"))?;
        let provided = info.dwFlags
            & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32
                | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32)
            != 0;
        let sample = if provided {
            None
        } else {
            let size = info.cbSize.max(
                self.width
                    .checked_mul(self.height)
                    .and_then(|pixels| pixels.checked_mul(3))
                    .map(|bytes| bytes / 2)
                    .ok_or_else(|| "H.264 output dimensions overflowed".to_string())?,
            );
            if u64::from(size) > self.maximum_frame_bytes {
                return Err("H.264 output frame exceeds worker limit".into());
            }
            Some(empty_sample(size)?)
        };
        let mut output = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(sample),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        };
        let mut status = 0_u32;
        let result = unsafe {
            self.transform
                .ProcessOutput(0, std::slice::from_mut(&mut output), &mut status)
        };
        let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
        let events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
        drop(events);
        match result {
            Ok(()) => {
                let sample = sample.ok_or("H.264 transform returned no output sample")?;
                let timestamp = unsafe { sample.GetSampleTime() }.unwrap_or(0);
                let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(0).max(0) as u64;
                if timestamp.unsigned_abs() > MAX_MEDIA_DURATION_100NS
                    || duration > MAX_MEDIA_DURATION_100NS
                {
                    return Err("H.264 output timestamp exceeds worker limits".into());
                }
                Ok(Pull::Frame(DecodedVideoSample {
                    bytes: copy_sample(&sample, "H.264 output", self.maximum_frame_bytes)?,
                    stride: self.stride,
                    timestamp_100ns: timestamp,
                    duration_100ns: duration,
                }))
            }
            Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(Pull::NeedInput),
            Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                set_nv12_output(&self.transform)?;
                self.pull()
            }
            Err(error) => Err(format!("decode H.264 output frame: {error}")),
        }
    }
}

enum Pull {
    Frame(DecodedVideoSample),
    NeedInput,
}

fn activate() -> Result<IMFTransform, String> {
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let mut pointer: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0_u32;
    unsafe {
        windows::Win32::Media::MediaFoundation::MFTEnumEx(
            MFT_CATEGORY_VIDEO_DECODER,
            MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER_WEB_ONLY,
            Some(&raw const input),
            Some(&raw const output),
            &mut pointer,
            &mut count,
        )
    }
    .map_err(|error| format!("enumerate H.264 transforms: {error}"))?;
    let activations = ActivationList { pointer, count };
    let activation = unsafe { std::slice::from_raw_parts(pointer, count as usize) }
        .first()
        .and_then(Option::as_ref)
        .ok_or_else(|| "Windows exposed no synchronous H.264 decoder".to_string())?;
    let transform = unsafe { activation.ActivateObject::<IMFTransform>() }
        .map_err(|error| format!("activate H.264 transform: {error}"));
    drop(activations);
    transform
}

fn input_type(track: &VideoTrack) -> Result<IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType() }
        .map_err(|error| format!("create H.264 input type: {error}"))?;
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .and_then(|_| media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264))
            .and_then(|_| {
                media_type.SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    (u64::from(track.width) << 32) | u64::from(track.height),
                )
            })
            .and_then(|_| {
                media_type.SetUINT32(
                    &MF_MT_INTERLACE_MODE,
                    MFVideoInterlace_MixedInterlaceOrProgressive.0 as u32,
                )
            })
            .and_then(|_| media_type.SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &track.sequence_header))
    }
    .map_err(|error| format!("populate H.264 input type: {error}"))?;
    Ok(media_type)
}

fn set_nv12_output(transform: &IMFTransform) -> Result<(), String> {
    for index in 0..64 {
        let Ok(candidate) = (unsafe { transform.GetOutputAvailableType(0, index) }) else {
            break;
        };
        let subtype = unsafe { candidate.GetGUID(&MF_MT_SUBTYPE) }
            .map_err(|error| format!("read H.264 output subtype: {error}"))?;
        if subtype == MFVideoFormat_NV12 {
            return unsafe { transform.SetOutputType(0, &candidate, 0) }
                .map_err(|error| format!("configure H.264 NV12 output: {error}"));
        }
    }
    Err("H.264 transform exposed no NV12 output".into())
}

fn media_sample(
    bytes: &[u8],
    timestamp: i64,
    duration: u64,
    key: bool,
) -> Result<IMFSample, String> {
    let length = u32::try_from(bytes.len()).map_err(|_| "H.264 access unit is too large")?;
    let buffer = unsafe { MFCreateMemoryBuffer(length) }
        .map_err(|error| format!("allocate H.264 input buffer: {error}"))?;
    let mut pointer = std::ptr::null_mut();
    unsafe { buffer.Lock(&mut pointer, None, None) }
        .map_err(|error| format!("lock H.264 input buffer: {error}"))?;
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len()) };
    unsafe {
        buffer
            .Unlock()
            .and_then(|_| buffer.SetCurrentLength(length))
    }
    .map_err(|error| format!("commit H.264 input buffer: {error}"))?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|error| format!("create H.264 input sample: {error}"))?;
    unsafe {
        sample
            .AddBuffer(&buffer)
            .and_then(|_| sample.SetSampleTime(timestamp))
            .and_then(|_| sample.SetSampleDuration(duration as i64))
            .and_then(|_| sample.SetUINT32(&MFSampleExtension_CleanPoint, u32::from(key)))
    }
    .map_err(|error| format!("populate H.264 input sample: {error}"))?;
    Ok(sample)
}

fn empty_sample(size: u32) -> Result<IMFSample, String> {
    let buffer = unsafe { MFCreateMemoryBuffer(size) }
        .map_err(|error| format!("allocate H.264 output buffer: {error}"))?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|error| format!("create H.264 output sample: {error}"))?;
    unsafe { sample.AddBuffer(&buffer) }
        .map_err(|error| format!("attach H.264 output buffer: {error}"))?;
    Ok(sample)
}

fn access_unit_summary(bytes: &[u8], length_size: usize) -> String {
    let mut offset = 0_usize;
    let mut types = Vec::new();
    while offset.saturating_add(length_size) <= bytes.len() && types.len() < 16 {
        let mut length = 0_usize;
        for byte in &bytes[offset..offset + length_size] {
            length = length.saturating_mul(256).saturating_add(*byte as usize);
        }
        offset += length_size;
        if length == 0 || length > bytes.len().saturating_sub(offset) {
            return format!("bytes={},invalid_nal_at={offset}", bytes.len());
        }
        types.push(bytes[offset] & 0x1f);
        offset += length;
    }
    format!("bytes={},nal_types={types:?}", bytes.len())
}
