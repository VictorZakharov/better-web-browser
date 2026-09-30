use crate::media_protocol::{MediaCodecFamily, MediaDecodeReport, MediaLimits};
use std::time::Instant;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MF_E_INVALIDSTREAMNUMBER, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
    MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateMFByteStreamOnStream, MFCreateMediaType,
    MFCreateSourceReaderFromByteStream, MFMediaType_Audio, MFMediaType_Video, MFSTARTUP_FULL,
    MFShutdown, MFStartup, MFVideoFormat_H264, MFVideoFormat_NV12,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, STREAM_SEEK_SET,
};
use windows::Win32::System::Variant::VT_I8;
use windows::core::GUID;

mod adaptive;
mod adaptive_audio;
mod append;
mod audio_only;
mod capabilities;
mod compressed_audio;
mod source;
use source::*;
mod audio;
mod flac;
mod fragmented_mp4;
mod h264;
mod ogg_vorbis;
mod playback;
mod stream;
mod video_buffer;
mod video_only;

pub(in crate::media_process) use adaptive_audio::AudioTrackReport;
pub(super) use append::decode_append;
pub(in crate::media_process) use audio::AudioDecoder;
use capabilities::ActivationList;
pub(super) use capabilities::probe;
pub(in crate::media_process) use playback::{DecodedVideoSample, VideoDecoder};
use stream::read_stream;

pub(super) struct DecodedMedia {
    pub(super) report: MediaDecodeReport,
    pub(super) playback: Option<VideoDecoder>,
}

pub(super) fn decode(bytes: &[u8], limits: MediaLimits) -> Result<DecodedMedia, String> {
    if flac::is_flac(bytes) {
        return flac::decode(bytes, limits, Instant::now());
    }
    if let Some(kind) = compressed_audio::classify(bytes) {
        return compressed_audio::decode(bytes, kind, limits, Instant::now());
    }
    if ogg_vorbis::is_ogg(bytes) {
        return ogg_vorbis::decode(bytes, limits, Instant::now());
    }
    decode_sources(bytes, bytes, bytes.len() as u64, limits)
}

pub(super) fn decode_tracks(
    video_bytes: &[u8],
    audio_bytes: &[u8],
    limits: MediaLimits,
) -> Result<DecodedMedia, String> {
    let encoded_bytes = video_bytes
        .len()
        .checked_add(audio_bytes.len())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| "adaptive media length overflowed".to_string())?;
    adaptive::decode(video_bytes, audio_bytes, encoded_bytes, limits)
}

fn decode_sources(
    video_bytes: &[u8],
    audio_bytes: &[u8],
    encoded_bytes: u64,
    limits: MediaLimits,
) -> Result<DecodedMedia, String> {
    let started = Instant::now();
    if video_bytes.is_empty()
        || audio_bytes.is_empty()
        || encoded_bytes == 0
        || encoded_bytes > limits.max_encoded_bytes
    {
        return Err("encoded media length exceeds worker limits".into());
    }
    let _apartment = ComApartment::initialize()
        .map_err(|status| format!("initialize media COM apartment: HRESULT {status:#x}"))?;
    let _foundation = MediaFoundation::start()
        .map_err(|status| format!("start Media Foundation: HRESULT {status:#x}"))?;

    let video_reader = source_reader(video_bytes)?;
    match unsafe {
        video_reader.GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, 0)
    } {
        Ok(_) => {}
        Err(error) if error.code() == MF_E_INVALIDSTREAMNUMBER => {
            // Only a missing video stream admits audio-only playback. A failed type
            // read for an existing stream must not silently change the source kind.
            return audio_only::decode(&video_reader, encoded_bytes, limits, started);
        }
        Err(error) => return Err(format!("read native video type: {error}")),
    }
    select_stream(
        &video_reader,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
        "video",
    )?;
    verify_native_type(
        &video_reader,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
        MFMediaType_Video,
        MFVideoFormat_H264,
        "H.264 video",
    )?;
    // NV12 is the H.264 decoder's native uncompressed output. Requiring RGB32 here would also
    // require a color-converter transform unrelated to proving demux and decode.
    let video_type = output_type(MFMediaType_Video, MFVideoFormat_NV12)?;
    unsafe {
        video_reader
            .SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &video_type,
            )
            .map_err(|error| format!("configure NV12 video output: {error}"))?;
    }
    let current_video = unsafe {
        video_reader
            .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
            .map_err(|error| format!("read decoded video format: {error}"))?
    };
    let frame_size = unsafe {
        current_video
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|error| format!("read decoded video dimensions: {error}"))?
    };
    let video_width = (frame_size >> 32) as u32;
    let video_height = frame_size as u32;
    let video = read_stream(
        &video_reader,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
        "video",
        limits.max_decoded_frame_bytes,
    )?;

    let audio_reader = source_reader(audio_bytes)?;
    match unsafe {
        audio_reader.GetNativeMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, 0)
    } {
        Ok(_) => {}
        Err(error) if error.code() == MF_E_INVALIDSTREAMNUMBER => {
            // A missing audio stream is a valid complete video resource. Other
            // Media Foundation failures must not silently turn into video-only playback.
            return video_only::decode(
                video_bytes,
                encoded_bytes,
                limits,
                started,
                video_width,
                video_height,
                video,
            );
        }
        Err(error) => return Err(format!("read native audio type: {error}")),
    }
    select_stream(
        &audio_reader,
        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
        "audio",
    )?;
    verify_native_type(
        &audio_reader,
        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
        MFMediaType_Audio,
        MFAudioFormat_AAC,
        "AAC audio",
    )?;
    let audio_type = output_type(MFMediaType_Audio, MFAudioFormat_PCM)?;
    unsafe {
        audio_reader
            .SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                None,
                &audio_type,
            )
            .map_err(|error| format!("configure PCM audio output: {error}"))?;
    }
    let current_audio = unsafe {
        audio_reader
            .GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
            .map_err(|error| format!("read decoded audio format: {error}"))?
    };
    let audio_sample_rate = unsafe {
        current_audio
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .map_err(|error| format!("read decoded audio sample rate: {error}"))?
    };
    let audio_channels = unsafe {
        current_audio
            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
            .map_err(|error| format!("read decoded audio channels: {error}"))?
    };
    let audio = read_stream(
        &audio_reader,
        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
        "audio",
        limits.max_decoded_frame_bytes,
    )?;
    if video.samples == 0
        || audio.samples == 0
        || video.samples as usize > crate::limits::MAX_MEDIA_DECODED_SAMPLES
        || audio.samples as usize > crate::limits::MAX_MEDIA_DECODED_SAMPLES
    {
        let native_video_samples = native_stream_summary(
            video_bytes,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
            "native video",
            limits,
        )
        .map(|summary| summary.samples.to_string())
        .unwrap_or_else(|error| format!("error:{error}"));
        return Err(format!(
            "decoded adaptive sample counts are invalid: video={} (native={native_video_samples}) from {} bytes [{}], audio={} from {} bytes [{}]",
            video.samples,
            video_bytes.len(),
            fragmented_mp4::summary(video_bytes),
            audio.samples,
            audio_bytes.len(),
            fragmented_mp4::summary(audio_bytes),
        ));
    }

    let report = MediaDecodeReport {
        buffered: crate::media_protocol::MediaBufferedExtent {
            video_start_100ns: video.first_timestamp.unwrap_or(0).max(0),
            video_end_100ns: video.end_100ns,
            audio_start_100ns: audio.first_timestamp.unwrap_or(0).max(0),
            audio_end_100ns: audio.end_100ns,
        },
        encoded_bytes,
        video_codec: MediaCodecFamily::H264,
        audio_codec: MediaCodecFamily::AacLc,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width,
        video_height,
        audio_sample_rate,
        audio_channels: u16::try_from(audio_channels)
            .map_err(|_| "decoded audio channel count is not representable".to_string())?,
        video_samples: video.samples,
        audio_samples: audio.samples,
        video_decoded_bytes: video.bytes,
        audio_decoded_bytes: audio.bytes,
        video_first_timestamp_100ns: video.first_timestamp.unwrap_or(0),
        video_last_timestamp_100ns: video.last_timestamp.unwrap_or(0),
        audio_first_timestamp_100ns: audio.first_timestamp.unwrap_or(0),
        audio_last_timestamp_100ns: audio.last_timestamp.unwrap_or(0),
        duration_100ns: video.end_100ns.max(audio.end_100ns),
        decode_micros: elapsed_micros(started),
    };
    report
        .validate(limits)
        .map_err(|error| format!("validate decoded media: {error}"))?;
    let playback = VideoDecoder::open(video_bytes, limits, report.video_samples)?;
    Ok(DecodedMedia {
        report,
        playback: Some(playback),
    })
}

fn native_stream_summary(
    bytes: &[u8],
    stream: u32,
    name: &str,
    limits: MediaLimits,
) -> Result<stream::StreamSummary, String> {
    let reader = source_reader(bytes)?;
    select_stream(&reader, stream, name)?;
    read_stream(&reader, stream, name, limits.max_decoded_frame_bytes)
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, i32> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if status.is_ok() {
            Ok(Self)
        } else {
            Err(status.0)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct MediaFoundation;

impl MediaFoundation {
    fn start() -> Result<Self, i32> {
        match unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            Ok(()) => Ok(Self),
            Err(error) => Err(error.code().0),
        }
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}
