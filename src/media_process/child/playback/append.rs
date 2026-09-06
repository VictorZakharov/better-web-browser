use super::*;

impl Playback {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::media_process::child) fn append_tracks(
        &mut self,
        source_id: u64,
        video_source_id: u64,
        audio_source_id: u64,
        video_length: u64,
        audio_length: u64,
        data_reader: &mut MediaDataReader<File>,
        limits: MediaLimits,
    ) -> Result<(u64, u64, crate::media_protocol::MediaBufferedExtent), String> {
        if self.pending.is_some() {
            return Err("media worker received an append before acknowledging its frame".into());
        }
        let Some((active_source_id, _)) = self.active.as_ref() else {
            return Err("media worker received an append with no active source".into());
        };
        if source_id != *active_source_id {
            return Err(format!(
                "stale media append for source {source_id}; expected {active_source_id}"
            ));
        }
        let expected_video_id = self
            .last_source_id
            .checked_add(1)
            .ok_or_else(|| "media source generation exhausted".to_string())?;
        let expected_audio_id = expected_video_id
            .checked_add(1)
            .ok_or_else(|| "media source generation exhausted".to_string())?;
        if video_source_id != expected_video_id || audio_source_id != expected_audio_id {
            return Err(format!(
                "stale adaptive append generation {video_source_id}/{audio_source_id}; expected {expected_video_id}/{expected_audio_id}"
            ));
        }
        let batch_bytes = video_length
            .checked_add(audio_length)
            .ok_or_else(|| "adaptive append length overflowed".to_string())?;
        if batch_bytes > limits.max_encoded_queue_bytes {
            return Err("adaptive append exceeds resident worker limits".into());
        }
        let total_bytes = self
            .encoded_bytes
            .checked_add(batch_bytes)
            .filter(|bytes| *bytes <= limits.max_encoded_bytes)
            .ok_or_else(|| "adaptive media exceeds total worker limits".to_string())?;
        let video_source =
            MediaSourceId::new(video_source_id).map_err(|error| error.to_string())?;
        let audio_source =
            MediaSourceId::new(audio_source_id).map_err(|error| error.to_string())?;
        let video_bytes = if video_length == 0 {
            Vec::new()
        } else {
            data_reader
                .read_source(video_source, video_length)
                .map_err(|error| format!("read appended video source: {error}"))?
        };
        let audio_bytes = if audio_length == 0 {
            Vec::new()
        } else {
            data_reader
                .read_source(audio_source, audio_length)
                .map_err(|error| format!("read appended audio source: {error}"))?
        };
        let decoded = backend::decode_append(&video_bytes, &audio_bytes, limits)?;
        let Some((_, active_video)) = self.active.as_mut() else {
            return Err("active video retired during adaptive append".into());
        };
        if let Some(video) = decoded.video {
            active_video.append(video)?;
        }
        let Some((_, active_audio)) = self.audio.as_ref() else {
            return Err("active audio retired during adaptive append".into());
        };
        if let Some(audio) = decoded.audio {
            active_audio.append(audio_bytes, audio)?;
        }
        self.last_source_id = audio_source_id;
        self.encoded_bytes = total_bytes;
        Ok((total_bytes, decoded.buffered.end_100ns(), decoded.buffered))
    }
}
