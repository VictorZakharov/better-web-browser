use super::super::super::backend::AudioDecoder;
use crate::limits::MAX_MEDIA_DECODED_AUDIO_QUEUE_BYTES;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::XAudio2::{
    IXAudio2, IXAudio2MasteringVoice, IXAudio2SourceVoice, IXAudio2VoiceCallback, XAUDIO2_BUFFER,
    XAUDIO2_COMMIT_NOW, XAUDIO2_DEFAULT_CHANNELS, XAUDIO2_DEFAULT_FREQ_RATIO,
    XAUDIO2_DEFAULT_PROCESSOR, XAUDIO2_DEFAULT_SAMPLERATE, XAUDIO2_VOICE_STATE,
    XAudio2CreateWithVersionInfo,
};
use windows::Win32::Media::Audio::{AudioCategory_Media, WAVE_FORMAT_PCM, WAVEFORMATEX};
use windows::core::PCWSTR;

const QUEUED_AUDIO_SAMPLES: usize = 4;
const NTDDI_WIN10: u32 = 0x0a00_0000;
// XAudio2 returns HRESULT_FROM_WIN32(ERROR_NOT_FOUND) when Windows has no default audio endpoint.
// Video playback must remain available in that environment, including on headless systems.
const AUDIO_ENDPOINT_NOT_FOUND: i32 = 0x8007_0490_u32 as i32;

#[derive(Clone, Copy)]
pub(super) struct OutputState {
    pub(super) position_100ns: u64,
    pub(super) playing: bool,
    pub(super) ended: bool,
}

pub(super) enum AudioOutput {
    Silent(SilentClock),
    Device(XAudioOutput),
}

impl AudioOutput {
    pub(super) fn silent() -> Self {
        Self::Silent(SilentClock::new())
    }

    pub(super) fn device_or_silent(sample_rate: u32, channels: u16) -> Result<Self, String> {
        match XAudioOutput::new(sample_rate, channels) {
            Ok(output) => Ok(Self::Device(output)),
            Err(DeviceOutputError::EndpointUnavailable) => Ok(Self::silent()),
            Err(DeviceOutputError::Fatal(error)) => Err(error),
        }
    }

    pub(super) fn playing(&self) -> bool {
        match self {
            Self::Silent(output) => output.playing,
            Self::Device(output) => output.playing,
        }
    }

    pub(super) fn set_playback(
        &mut self,
        playing: bool,
        volume_millis: u16,
        decoder: &mut AudioDecoder,
    ) -> Result<(), String> {
        match self {
            Self::Silent(output) => {
                output.set_playback(playing);
                Ok(())
            }
            Self::Device(output) => output.set_playback(playing, volume_millis, decoder),
        }
    }

    pub(super) fn state(&mut self, decoder: &mut AudioDecoder) -> Result<OutputState, String> {
        match self {
            Self::Silent(output) => Ok(output.state()),
            Self::Device(output) => output.state(decoder),
        }
    }

    pub(super) fn seek(
        &mut self,
        position_100ns: u64,
        decoder: &mut AudioDecoder,
    ) -> Result<(), String> {
        match self {
            Self::Silent(output) => {
                output.seek(position_100ns);
                Ok(())
            }
            Self::Device(output) => output.seek(position_100ns, decoder),
        }
    }
}

pub(super) struct SilentClock {
    elapsed: Duration,
    started: Option<Instant>,
    playing: bool,
}

impl SilentClock {
    fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            started: None,
            playing: false,
        }
    }

    fn set_playback(&mut self, playing: bool) {
        if self.playing == playing {
            return;
        }
        if playing {
            self.started = Some(Instant::now());
        } else if let Some(started) = self.started.take() {
            self.elapsed = self.elapsed.saturating_add(started.elapsed());
        }
        self.playing = playing;
    }

    fn state(&self) -> OutputState {
        let elapsed = self.started.map_or(self.elapsed, |started| {
            self.elapsed.saturating_add(started.elapsed())
        });
        OutputState {
            position_100ns: elapsed
                .as_nanos()
                .saturating_div(100)
                .min(u128::from(u64::MAX)) as u64,
            playing: self.playing,
            ended: false,
        }
    }

    fn seek(&mut self, position_100ns: u64) {
        self.elapsed = Duration::from_nanos(position_100ns.saturating_mul(100));
        self.started = self.playing.then(Instant::now);
    }
}

pub(super) struct XAudioOutput {
    engine: IXAudio2,
    mastering: IXAudio2MasteringVoice,
    source: IXAudio2SourceVoice,
    sample_rate: u32,
    queued: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    input_ended: bool,
    playing: bool,
    position_base_100ns: u64,
    sample_origin: u64,
}

enum DeviceOutputError {
    EndpointUnavailable,
    Fatal(String),
}

impl XAudioOutput {
    fn new(sample_rate: u32, channels: u16) -> Result<Self, DeviceOutputError> {
        let format = pcm_format(sample_rate, channels).map_err(DeviceOutputError::Fatal)?;
        let mut engine = None;
        unsafe {
            XAudio2CreateWithVersionInfo(&mut engine, 0, XAUDIO2_DEFAULT_PROCESSOR, NTDDI_WIN10)
        }
        .map_err(|error| DeviceOutputError::Fatal(format!("create XAudio2 engine: {error}")))?;
        let engine = engine
            .ok_or_else(|| DeviceOutputError::Fatal("XAudio2 returned no engine".to_string()))?;
        let mut mastering = None;
        if let Err(error) = unsafe {
            engine.CreateMasteringVoice(
                &mut mastering,
                XAUDIO2_DEFAULT_CHANNELS,
                XAUDIO2_DEFAULT_SAMPLERATE,
                0,
                PCWSTR::null(),
                None,
                AudioCategory_Media,
            )
        } {
            unsafe { engine.StopEngine() };
            return if missing_default_audio_endpoint(error.code()) {
                Err(DeviceOutputError::EndpointUnavailable)
            } else {
                Err(DeviceOutputError::Fatal(format!(
                    "create XAudio2 mastering voice: {error}"
                )))
            };
        }
        let Some(mastering) = mastering else {
            unsafe { engine.StopEngine() };
            return Err(DeviceOutputError::Fatal(
                "XAudio2 returned no mastering voice".to_string(),
            ));
        };
        let mut source = None;
        if let Err(error) = unsafe {
            engine.CreateSourceVoice(
                &mut source,
                &raw const format,
                0,
                XAUDIO2_DEFAULT_FREQ_RATIO,
                None::<&IXAudio2VoiceCallback>,
                None,
                None,
            )
        } {
            unsafe {
                mastering.DestroyVoice();
                engine.StopEngine();
            }
            return Err(DeviceOutputError::Fatal(format!(
                "create XAudio2 source voice: {error}"
            )));
        }
        let Some(source) = source else {
            unsafe {
                mastering.DestroyVoice();
                engine.StopEngine();
            }
            return Err(DeviceOutputError::Fatal(
                "XAudio2 returned no source voice".to_string(),
            ));
        };
        Ok(Self {
            engine,
            mastering,
            source,
            sample_rate,
            queued: VecDeque::with_capacity(QUEUED_AUDIO_SAMPLES),
            queued_bytes: 0,
            input_ended: false,
            playing: false,
            position_base_100ns: 0,
            sample_origin: 0,
        })
    }

    fn set_playback(
        &mut self,
        playing: bool,
        volume_millis: u16,
        decoder: &mut AudioDecoder,
    ) -> Result<(), String> {
        unsafe {
            self.source
                .SetVolume(f32::from(volume_millis) / 1_000.0, XAUDIO2_COMMIT_NOW)
        }
        .map_err(|error| format!("set XAudio2 volume: {error}"))?;
        if playing == self.playing {
            return Ok(());
        }
        if playing {
            self.pump(decoder)?;
            unsafe { self.source.Start(0, XAUDIO2_COMMIT_NOW) }
                .map_err(|error| format!("start XAudio2 source voice: {error}"))?;
        } else {
            unsafe { self.source.Stop(0, XAUDIO2_COMMIT_NOW) }
                .map_err(|error| format!("pause XAudio2 source voice: {error}"))?;
        }
        self.playing = playing;
        Ok(())
    }

    fn state(&mut self, decoder: &mut AudioDecoder) -> Result<OutputState, String> {
        self.pump(decoder)?;
        let state = self.voice_state();
        let ended = self.input_ended && state.BuffersQueued == 0;
        if ended {
            self.playing = false;
        }
        Ok(OutputState {
            position_100ns: self.position_base_100ns.saturating_add(
                state
                    .SamplesPlayed
                    .saturating_sub(self.sample_origin)
                    .saturating_mul(10_000_000)
                    .checked_div(u64::from(self.sample_rate))
                    .unwrap_or_default(),
            ),
            playing: self.playing,
            ended,
        })
    }

    fn seek(&mut self, position_100ns: u64, decoder: &mut AudioDecoder) -> Result<(), String> {
        let resume = self.playing;
        if resume {
            unsafe { self.source.Stop(0, XAUDIO2_COMMIT_NOW) }
                .map_err(|error| format!("stop XAudio2 for seek: {error}"))?;
        }
        unsafe { self.source.FlushSourceBuffers() }
            .map_err(|error| format!("flush XAudio2 for seek: {error}"))?;
        self.queued.clear();
        self.queued_bytes = 0;
        self.input_ended = false;
        self.position_base_100ns = position_100ns;
        self.sample_origin = self.voice_state().SamplesPlayed;
        if resume {
            self.pump(decoder)?;
            unsafe { self.source.Start(0, XAUDIO2_COMMIT_NOW) }
                .map_err(|error| format!("resume XAudio2 after seek: {error}"))?;
        }
        Ok(())
    }

    fn pump(&mut self, decoder: &mut AudioDecoder) -> Result<(), String> {
        let state = self.voice_state();
        while self.queued.len() > state.BuffersQueued as usize {
            if let Some(bytes) = self.queued.pop_front() {
                self.queued_bytes = self.queued_bytes.saturating_sub(bytes.len());
            }
        }
        while !self.input_ended && self.queued.len() < QUEUED_AUDIO_SAMPLES {
            let Some(bytes) = decoder.next_sample()? else {
                self.input_ended = true;
                break;
            };
            if bytes.is_empty() {
                continue;
            }
            let next_bytes = self
                .queued_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| "decoded PCM queue size overflow".to_string())?;
            if next_bytes > MAX_MEDIA_DECODED_AUDIO_QUEUE_BYTES {
                return Err("decoded PCM queue exceeds worker limit".into());
            }
            let buffer = XAUDIO2_BUFFER {
                AudioBytes: u32::try_from(bytes.len())
                    .map_err(|_| "decoded PCM sample is too large".to_string())?,
                pAudioData: bytes.as_ptr(),
                ..Default::default()
            };
            unsafe { self.source.SubmitSourceBuffer(&buffer, None) }
                .map_err(|error| format!("queue XAudio2 PCM sample: {error}"))?;
            self.queued_bytes = next_bytes;
            self.queued.push_back(bytes);
        }
        Ok(())
    }

    fn voice_state(&self) -> XAUDIO2_VOICE_STATE {
        let mut state = XAUDIO2_VOICE_STATE::default();
        unsafe { self.source.GetState(&mut state, 0) };
        state
    }
}

impl Drop for XAudioOutput {
    fn drop(&mut self) {
        unsafe {
            self.source.DestroyVoice();
            self.mastering.DestroyVoice();
            self.engine.StopEngine();
        }
    }
}

fn pcm_format(sample_rate: u32, channels: u16) -> Result<WAVEFORMATEX, String> {
    let block_align = channels
        .checked_mul(2)
        .ok_or_else(|| "PCM block alignment overflow".to_string())?;
    Ok(WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate
            .checked_mul(u32::from(block_align))
            .ok_or_else(|| "PCM byte rate overflow".to_string())?,
        nBlockAlign: block_align,
        wBitsPerSample: 16,
        cbSize: 0,
    })
}

fn missing_default_audio_endpoint(code: windows::core::HRESULT) -> bool {
    code.0 == AUDIO_ENDPOINT_NOT_FOUND
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_default_endpoint_selects_silent_output() {
        assert!(missing_default_audio_endpoint(windows::core::HRESULT(
            AUDIO_ENDPOINT_NOT_FOUND
        )));
        assert!(!missing_default_audio_endpoint(windows::core::HRESULT(
            0x8007_0057_u32 as i32
        )));
    }
}
