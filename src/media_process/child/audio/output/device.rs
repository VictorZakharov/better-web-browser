use super::*;
impl XAudioOutput {
    pub(super) fn new(sample_rate: u32, channels: u16) -> Result<Self, DeviceOutputError> {
        let format = pcm_format(sample_rate, channels).map_err(DeviceOutputError::Fatal)?;
        let apartment =
            crate::media_process::backend::ComApartment::initialize().map_err(|status| {
                DeviceOutputError::Fatal(format!(
                    "initialize audio output COM apartment: HRESULT {status:#x}"
                ))
            })?;
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
        let output = Self {
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
            graph_frames_submitted: 0,
            graph_underruns: 0,
            _apartment: apartment,
        };
        // A hidden device probe exercises the real voice and clock while
        // muting at the final source voice, including subsequent media volume edits.
        unsafe {
            output
                .source
                .SetVolume(output_volume(1.0), XAUDIO2_COMMIT_NOW)
        }
        .map_err(|error| {
            DeviceOutputError::Fatal(format!("set initial XAudio2 volume: {error}"))
        })?;
        Ok(output)
    }
}
