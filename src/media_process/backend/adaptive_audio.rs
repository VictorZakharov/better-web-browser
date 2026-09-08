use super::*;

#[derive(Clone, Copy)]
pub(in crate::media_process) struct AudioTrackReport {
    pub audio_samples: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub audio_first_timestamp_100ns: i64,
    pub duration_100ns: u64,
}

impl From<MediaDecodeReport> for AudioTrackReport {
    fn from(report: MediaDecodeReport) -> Self {
        Self {
            audio_samples: report.audio_samples,
            audio_sample_rate: report.audio_sample_rate,
            audio_channels: report.audio_channels,
            audio_first_timestamp_100ns: report.audio_first_timestamp_100ns,
            duration_100ns: report.buffered.audio_end_100ns,
        }
    }
}

pub(super) fn inspect(
    audio_bytes: &[u8],
    limits: MediaLimits,
) -> Result<(AudioTrackReport, stream::StreamSummary), String> {
    let _apartment = ComApartment::initialize().map_err(|status| {
        format!("initialize adaptive audio COM apartment: HRESULT {status:#x}")
    })?;
    let _foundation = MediaFoundation::start()
        .map_err(|status| format!("start adaptive audio Media Foundation: HRESULT {status:#x}"))?;
    let audio_reader = source_reader(audio_bytes)?;
    select_stream(
        &audio_reader,
        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
        "adaptive audio",
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
            .map_err(|error| format!("configure adaptive PCM audio output: {error}"))?;
    }
    let current_audio = unsafe {
        audio_reader
            .GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
            .map_err(|error| format!("read adaptive audio format: {error}"))?
    };
    let audio_sample_rate = unsafe {
        current_audio
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .map_err(|error| format!("read adaptive audio sample rate: {error}"))?
    };
    let audio_channels = unsafe {
        current_audio
            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
            .map_err(|error| format!("read adaptive audio channels: {error}"))?
    };
    let audio = read_stream(
        &audio_reader,
        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
        "adaptive audio",
        limits.max_decoded_frame_bytes,
    )?;
    if audio.samples == 0 {
        return Err("adaptive audio stream produced no samples".into());
    }
    if audio_sample_rate == 0
        || audio_sample_rate > 384_000
        || audio_channels == 0
        || audio_channels > 32
    {
        return Err("adaptive audio format exceeds worker limits".into());
    }
    let report = AudioTrackReport {
        audio_samples: audio.samples,
        audio_sample_rate,
        audio_channels: audio_channels as u16,
        audio_first_timestamp_100ns: audio.first_timestamp.unwrap_or(0),
        duration_100ns: audio.end_100ns,
    };
    Ok((report, audio))
}
