use super::{
    ComApartment, MediaFoundation, output_type, source_reader, stream::copy_sample,
    verify_native_type,
};
use crate::limits::{
    MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES, MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DURATION_100NS,
};
use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MFAudioFormat_AAC, MFAudioFormat_PCM,
    MFMediaType_Audio,
};

const PCM_BITS_PER_SAMPLE: u32 = 16;

/// Pull-driven AAC-to-PCM decoder owned exclusively by the restricted media process.
pub(in crate::media_process) struct AudioDecoder {
    reader: IMFSourceReader,
    sample_rate: u32,
    channels: u16,
    remaining_samples: u32,
    maximum_sample_bytes: u64,
    last_timestamp: Option<i64>,
    _foundation: MediaFoundation,
    _apartment: ComApartment,
}

impl AudioDecoder {
    pub(in crate::media_process) fn open(
        bytes: &[u8],
        expected_samples: u32,
        expected_sample_rate: u32,
        expected_channels: u16,
    ) -> Result<Self, String> {
        if expected_samples == 0 || expected_samples as usize > MAX_MEDIA_DECODED_SAMPLES {
            return Err("decoded audio sample count exceeds worker limit".into());
        }
        let apartment = ComApartment::initialize()
            .map_err(|status| format!("initialize audio COM apartment: HRESULT {status:#x}"))?;
        let foundation = MediaFoundation::start()
            .map_err(|status| format!("start audio Media Foundation: HRESULT {status:#x}"))?;
        let reader = source_reader(bytes)?;
        verify_native_type(
            &reader,
            MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
            MFMediaType_Audio,
            MFAudioFormat_AAC,
            "AAC audio",
        )?;
        let audio_type = output_type(MFMediaType_Audio, MFAudioFormat_PCM)?;
        unsafe {
            reader
                .SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                    None,
                    &audio_type,
                )
                .map_err(|error| format!("configure playback PCM output: {error}"))?;
        }
        let current = unsafe {
            reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
                .map_err(|error| format!("read playback audio format: {error}"))?
        };
        let sample_rate = unsafe {
            current
                .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                .map_err(|error| format!("read playback audio sample rate: {error}"))?
        };
        let channels = unsafe {
            current
                .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
                .map_err(|error| format!("read playback audio channels: {error}"))?
        };
        let bits = unsafe {
            current
                .GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)
                .unwrap_or(PCM_BITS_PER_SAMPLE)
        };
        if sample_rate != expected_sample_rate
            || channels != u32::from(expected_channels)
            || bits != PCM_BITS_PER_SAMPLE
        {
            return Err("playback PCM format disagreed with the decode report".into());
        }
        Ok(Self {
            reader,
            sample_rate,
            channels: expected_channels,
            remaining_samples: expected_samples,
            maximum_sample_bytes: MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES as u64,
            last_timestamp: None,
            _foundation: foundation,
            _apartment: apartment,
        })
    }

    pub(in crate::media_process) const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub(in crate::media_process) const fn channels(&self) -> u16 {
        self.channels
    }

    pub(in crate::media_process) fn next_sample(&mut self) -> Result<Option<Vec<u8>>, String> {
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
                        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut timestamp),
                        Some(&mut sample),
                    )
                    .map_err(|error| format!("decode playback audio sample: {error}"))?;
            }
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err("Media Foundation reported a playback audio error".into());
            }
            if let Some(sample) = sample {
                if timestamp.unsigned_abs() > MAX_MEDIA_DURATION_100NS
                    || self.last_timestamp.is_some_and(|last| timestamp < last)
                {
                    return Err("playback audio timestamp is invalid".into());
                }
                self.last_timestamp = Some(timestamp);
                self.remaining_samples -= 1;
                return copy_sample(&sample, "playback audio", self.maximum_sample_bytes).map(Some);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                self.remaining_samples = 0;
                return Ok(None);
            }
        }
    }
}
