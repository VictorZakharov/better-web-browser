use super::{
    ComApartment, MediaFoundation, fragmented_mp4::VideoTrack, h264::TransformVideoDecoder,
    output_type, seek_source_reader, select_stream, source_reader, stream::copy_sample,
    verify_native_type,
};
use crate::limits::{MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DURATION_100NS};
use crate::media_protocol::MediaLimits;
use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MF_MT_FRAME_SIZE, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MFGetStrideForBitmapInfoHeader,
    MFMediaType_Video, MFVideoFormat_H264, MFVideoFormat_NV12,
};

pub(in crate::media_process) struct DecodedVideoSample {
    pub(in crate::media_process) bytes: Vec<u8>,
    pub(in crate::media_process) stride: u32,
    pub(in crate::media_process) timestamp_100ns: i64,
    pub(in crate::media_process) duration_100ns: u64,
}

pub(in crate::media_process) struct VideoDecoder {
    inner: Decoder,
}

enum Decoder {
    SourceReader(SourceReaderVideoDecoder),
    Transform(TransformVideoDecoder),
}

impl VideoDecoder {
    pub(super) fn open(
        bytes: &[u8],
        limits: MediaLimits,
        expected_samples: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            inner: Decoder::SourceReader(SourceReaderVideoDecoder::open(
                bytes,
                limits,
                expected_samples,
            )?),
        })
    }

    pub(super) fn open_fragmented(track: VideoTrack, limits: MediaLimits) -> Result<Self, String> {
        Ok(Self {
            inner: Decoder::Transform(TransformVideoDecoder::open(track, limits)?),
        })
    }

    pub(in crate::media_process) fn dimensions(&self) -> (u32, u32) {
        match &self.inner {
            Decoder::SourceReader(decoder) => decoder.dimensions(),
            Decoder::Transform(decoder) => decoder.dimensions(),
        }
    }

    pub(in crate::media_process) fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        match &mut self.inner {
            Decoder::SourceReader(decoder) => decoder.seek(position_100ns),
            Decoder::Transform(decoder) => decoder.seek(position_100ns),
        }
    }

    pub(in crate::media_process) fn next_frame(
        &mut self,
    ) -> Result<Option<DecodedVideoSample>, String> {
        match &mut self.inner {
            Decoder::SourceReader(decoder) => decoder.next_frame(),
            Decoder::Transform(decoder) => decoder.next_frame(),
        }
    }
}

/// Pull-driven video decode. A frame acknowledgement is the queue bound: the worker never
/// advances this decoder while an earlier decoded frame is outstanding.
struct SourceReaderVideoDecoder {
    reader: IMFSourceReader,
    width: u32,
    height: u32,
    stride: u32,
    maximum_frame_bytes: u64,
    remaining_samples: u32,
    total_samples: u32,
    last_timestamp: Option<i64>,
    _foundation: MediaFoundation,
    _apartment: ComApartment,
}

impl SourceReaderVideoDecoder {
    fn open(bytes: &[u8], limits: MediaLimits, expected_samples: u32) -> Result<Self, String> {
        if expected_samples == 0 || expected_samples as usize > MAX_MEDIA_DECODED_SAMPLES {
            return Err("decoded video sample count exceeds worker limit".into());
        }
        let apartment = ComApartment::initialize()
            .map_err(|status| format!("initialize playback COM apartment: HRESULT {status:#x}"))?;
        let foundation = MediaFoundation::start()
            .map_err(|status| format!("start playback Media Foundation: HRESULT {status:#x}"))?;
        let reader = source_reader(bytes)?;
        select_stream(
            &reader,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
            "playback video",
        )?;
        verify_native_type(
            &reader,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
            MFMediaType_Video,
            MFVideoFormat_H264,
            "H.264 video",
        )?;
        let video_type = output_type(MFMediaType_Video, MFVideoFormat_NV12)?;
        unsafe {
            reader
                .SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    None,
                    &video_type,
                )
                .map_err(|error| format!("configure playback NV12 output: {error}"))?;
        }
        let current = unsafe {
            reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                .map_err(|error| format!("read playback video format: {error}"))?
        };
        let frame_size = unsafe {
            current
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|error| format!("read playback video dimensions: {error}"))?
        };
        let width = (frame_size >> 32) as u32;
        let height = frame_size as u32;
        if width == 0
            || height == 0
            || width > limits.max_dimension
            || height > limits.max_dimension
        {
            return Err("playback video dimensions exceed worker limits".into());
        }
        let stride = unsafe { MFGetStrideForBitmapInfoHeader(MFVideoFormat_NV12.data1, width) }
            .map_err(|error| format!("read playback NV12 stride: {error}"))?;
        let stride =
            u32::try_from(stride).map_err(|_| "playback NV12 stride is negative".to_string())?;
        Ok(Self {
            reader,
            width,
            height,
            stride,
            maximum_frame_bytes: limits.max_decoded_frame_bytes,
            remaining_samples: expected_samples,
            total_samples: expected_samples,
            last_timestamp: None,
            _foundation: foundation,
            _apartment: apartment,
        })
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        seek_source_reader(&self.reader, position_100ns)?;
        self.remaining_samples = self.total_samples;
        self.last_timestamp = None;
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Option<DecodedVideoSample>, String> {
        if self.remaining_samples == 0 {
            return Ok(None);
        }
        loop {
            let mut flags = 0_u32;
            let mut timestamp = 0_i64;
            let mut sample = None;
            unsafe {
                self.reader
                    .ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut timestamp),
                        Some(&mut sample),
                    )
                    .map_err(|error| format!("decode playback video sample: {error}"))?;
            }
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err("Media Foundation reported a playback video error".into());
            }
            if let Some(sample) = sample {
                if timestamp.unsigned_abs() > MAX_MEDIA_DURATION_100NS
                    || self.last_timestamp.is_some_and(|last| timestamp < last)
                {
                    return Err("playback video timestamp is invalid".into());
                }
                let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(0).max(0) as u64;
                if duration > MAX_MEDIA_DURATION_100NS {
                    return Err("playback video duration exceeds worker limit".into());
                }
                self.last_timestamp = Some(timestamp);
                self.remaining_samples -= 1;
                return Ok(Some(DecodedVideoSample {
                    bytes: copy_sample(&sample, "playback video", self.maximum_frame_bytes)?,
                    stride: self.stride,
                    timestamp_100ns: timestamp,
                    duration_100ns: duration,
                }));
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                self.remaining_samples = 0;
                return Ok(None);
            }
        }
    }
}
