use super::*;

pub(super) fn decode(
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
        return Err("encoded adaptive media length exceeds worker limits".into());
    }
    let video_track = fragmented_mp4::parse_video(video_bytes, limits)?;
    let video_width = video_track.width;
    let video_height = video_track.height;
    let video_samples = u32::try_from(video_track.samples.len())
        .map_err(|_| "adaptive video sample count is not representable")?;
    let video_decoded_bytes = video_track.decoded_bytes()?;
    let video_first_timestamp = video_track
        .samples
        .iter()
        .map(|sample| sample.timestamp_100ns)
        .min()
        .unwrap_or(0);
    let video_last_timestamp = video_track
        .samples
        .iter()
        .map(|sample| sample.timestamp_100ns)
        .max()
        .unwrap_or(0);
    let video_duration = video_track.duration_100ns();

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
    let playback = VideoDecoder::open_fragmented(video_track, limits)?;
    let report = MediaDecodeReport {
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
            .map_err(|_| "adaptive audio channel count is not representable")?,
        video_samples,
        audio_samples: audio.samples,
        video_decoded_bytes,
        audio_decoded_bytes: audio.bytes,
        video_first_timestamp_100ns: video_first_timestamp,
        video_last_timestamp_100ns: video_last_timestamp,
        audio_first_timestamp_100ns: audio.first_timestamp.unwrap_or(0),
        audio_last_timestamp_100ns: audio.last_timestamp.unwrap_or(0),
        duration_100ns: video_duration.max(audio.end_100ns),
        decode_micros: elapsed_micros(started),
    };
    report
        .validate(limits)
        .map_err(|error| format!("validate adaptive decoded media: {error}"))?;
    Ok(DecodedMedia { report, playback })
}
