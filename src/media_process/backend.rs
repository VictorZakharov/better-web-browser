use crate::media_protocol::{
    MediaCapabilityReport, MediaCodecFamily, MediaDecodeReport, MediaLimits,
};
use std::ptr::null_mut;
use std::time::Instant;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFSourceReader, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION, MFAudioFormat_AAC, MFAudioFormat_PCM,
    MFCreateMFByteStreamOnStream, MFCreateMediaType, MFCreateSourceReaderFromByteStream,
    MFMediaType_Audio, MFMediaType_Video, MFSTARTUP_FULL, MFShutdown, MFStartup,
    MFT_CATEGORY_AUDIO_DECODER, MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_FLAG_ALL,
    MFT_ENUM_FLAG_SORTANDFILTER_WEB_ONLY, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
    MFVideoFormat_NV12,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, STREAM_SEEK_SET,
};
use windows::Win32::System::Variant::VT_I8;
use windows::core::GUID;

mod audio;
mod playback;
mod stream;

pub(in crate::media_process) use audio::AudioDecoder;
pub(in crate::media_process) use playback::VideoDecoder;
use stream::read_stream;

pub(super) struct DecodedMedia {
    pub(super) report: MediaDecodeReport,
    pub(super) playback: VideoDecoder,
}

pub(super) fn probe(limits: MediaLimits) -> MediaCapabilityReport {
    let started = Instant::now();
    let _apartment = match ComApartment::initialize() {
        Ok(apartment) => apartment,
        Err(status) => return failed_report(status, started),
    };
    let _foundation = match MediaFoundation::start() {
        Ok(foundation) => foundation,
        Err(status) => return failed_report(status, started),
    };
    let (h264_hresult, h264_decoders) = enumerate_decoders(
        MFT_CATEGORY_VIDEO_DECODER,
        MFMediaType_Video,
        MFVideoFormat_H264,
        limits.max_decoder_candidates,
    );
    let (aac_hresult, aac_decoders) = enumerate_decoders(
        MFT_CATEGORY_AUDIO_DECODER,
        MFMediaType_Audio,
        MFAudioFormat_AAC,
        limits.max_decoder_candidates,
    );
    MediaCapabilityReport {
        startup_hresult: 0,
        h264_hresult,
        aac_hresult,
        h264_decoders,
        aac_decoders,
        probe_micros: elapsed_micros(started),
    }
}

pub(super) fn decode(bytes: &[u8], limits: MediaLimits) -> Result<DecodedMedia, String> {
    let started = Instant::now();
    if bytes.is_empty() || bytes.len() as u64 > limits.max_encoded_bytes {
        return Err("encoded media length exceeds worker limits".into());
    }
    let _apartment = ComApartment::initialize()
        .map_err(|status| format!("initialize media COM apartment: HRESULT {status:#x}"))?;
    let _foundation = MediaFoundation::start()
        .map_err(|status| format!("start Media Foundation: HRESULT {status:#x}"))?;

    let video_reader = source_reader(bytes)?;
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

    let audio_reader = source_reader(bytes)?;
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

    let report = MediaDecodeReport {
        encoded_bytes: bytes.len() as u64,
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
    let playback = VideoDecoder::open(bytes, limits, report.video_samples)?;
    Ok(DecodedMedia { report, playback })
}

fn source_reader(bytes: &[u8]) -> Result<IMFSourceReader, String> {
    let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }
        .map_err(|error| format!("create in-memory media stream: {error}"))?;
    let mut written = 0_u32;
    unsafe {
        stream
            .Write(
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                Some(&mut written),
            )
            .ok()
            .map_err(|error| format!("copy encoded media into memory stream: {error}"))?;
    }
    if written as usize != bytes.len() {
        return Err("in-memory media stream accepted a partial write".into());
    }
    unsafe {
        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(|error| format!("rewind in-memory media stream: {error}"))?;
    }
    let byte_stream = unsafe { MFCreateMFByteStreamOnStream(&stream) }
        .map_err(|error| format!("adapt memory stream for Media Foundation: {error}"))?;
    unsafe { MFCreateSourceReaderFromByteStream(&byte_stream, None) }
        .map_err(|error| format!("create Media Foundation Source Reader: {error}"))
}

fn seek_source_reader(reader: &IMFSourceReader, position_100ns: u64) -> Result<(), String> {
    let position = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_I8,
                Anonymous: PROPVARIANT_0_0_0 {
                    hVal: position_100ns as i64,
                },
                ..Default::default()
            }),
        },
    };
    unsafe {
        reader
            .SetCurrentPosition(&GUID::zeroed(), &position)
            .map_err(|error| format!("seek Media Foundation Source Reader: {error}"))
    }
}

fn verify_native_type(
    reader: &IMFSourceReader,
    stream: u32,
    expected_major: GUID,
    expected_subtype: GUID,
    name: &str,
) -> Result<(), String> {
    let native = unsafe { reader.GetNativeMediaType(stream, 0) }
        .map_err(|error| format!("read native {name} type: {error}"))?;
    let major = unsafe { native.GetGUID(&MF_MT_MAJOR_TYPE) }
        .map_err(|error| format!("read native {name} major type: {error}"))?;
    let subtype = unsafe { native.GetGUID(&MF_MT_SUBTYPE) }
        .map_err(|error| format!("read native {name} subtype: {error}"))?;
    if major != expected_major || subtype != expected_subtype {
        return Err(format!(
            "owned fixture did not expose expected {name} track"
        ));
    }
    Ok(())
}

fn output_type(
    major: GUID,
    subtype: GUID,
) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType() }
        .map_err(|error| format!("create decoded output type: {error}"))?;
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &major)
            .and_then(|_| media_type.SetGUID(&MF_MT_SUBTYPE, &subtype))
            .map_err(|error| format!("configure decoded output type: {error}"))?;
    }
    Ok(media_type)
}

fn failed_report(status: i32, started: Instant) -> MediaCapabilityReport {
    MediaCapabilityReport {
        startup_hresult: status,
        h264_hresult: status,
        aac_hresult: status,
        h264_decoders: 0,
        aac_decoders: 0,
        probe_micros: elapsed_micros(started),
    }
}

fn enumerate_decoders(category: GUID, major: GUID, subtype: GUID, maximum: u16) -> (i32, u16) {
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: subtype,
    };
    let mut pointer: *mut Option<IMFActivate> = null_mut();
    let mut count = 0_u32;
    let flags = MFT_ENUM_FLAG_ALL | MFT_ENUM_FLAG_SORTANDFILTER_WEB_ONLY;
    let result = unsafe {
        MFTEnumEx(
            category,
            flags,
            Some(&raw const input),
            None,
            &mut pointer,
            &mut count,
        )
    };
    let _activations = ActivationList { pointer, count };
    match result {
        Ok(()) => (0, count.min(u32::from(maximum)) as u16),
        Err(error) => (error.code().0, 0),
    }
    // `activations` releases every IMFActivate and the COM-allocated array here.
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
