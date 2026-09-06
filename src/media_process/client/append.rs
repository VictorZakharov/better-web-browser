use super::*;

impl MediaClient {
    pub(crate) fn append_tracks(
        &mut self,
        source_id: u64,
        video_bytes: &[u8],
        audio_bytes: &[u8],
        mut progress: impl FnMut() -> Result<(), String>,
    ) -> Result<(u64, u64, crate::media_protocol::MediaBufferedExtent), String> {
        let encoded_length = video_bytes
            .len()
            .checked_add(audio_bytes.len())
            .ok_or_else(|| "adaptive append length overflowed".to_string())?;
        if source_id == 0
            || encoded_length == 0
            || encoded_length as u64 > self.limits.max_encoded_queue_bytes
        {
            return Err("adaptive append exceeds the contained worker queue limit".into());
        }
        let request_id = self.allocate_request()?;
        let video_source_id = self.next_source;
        let audio_source_id = checked_next(video_source_id, "media source identity")?;
        self.next_source = checked_next(audio_source_id, "media source identity")?;
        self.send(BrowserMediaMessage::AppendTracks {
            request_id,
            source_id,
            video_source_id,
            audio_source_id,
            video_length: video_bytes.len() as u64,
            audio_length: audio_bytes.len() as u64,
        })?;
        let video_source =
            MediaSourceId::new(video_source_id).map_err(|error| error.to_string())?;
        let audio_source =
            MediaSourceId::new(audio_source_id).map_err(|error| error.to_string())?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = self.session;
        let nonce = self.nonce;
        let sent = std::thread::scope(|scope| {
            let sender = scope.spawn(move || {
                let mut writer = MediaDataWriter::new(output, session, nonce);
                if !video_bytes.is_empty() {
                    writer.send_source(video_source, video_bytes)?;
                }
                if !audio_bytes.is_empty() {
                    writer.send_source(audio_source, audio_bytes)?;
                }
                Ok::<_, crate::media_data_protocol::MediaDataError>(())
            });
            let response = self.receive_with_progress("append adaptive tracks", &mut progress)?;
            let sent = sender
                .join()
                .map_err(|_| "media data writer panicked".to_string())?
                .map_err(|error| format!("deliver adaptive media append: {error}"));
            Ok::<_, String>((response, sent))
        })?;
        sent.1?;
        match sent.0 {
            WorkerMediaMessage::Appended {
                buffered,
                request_id: actual,
                source_id: actual_source,
                encoded_bytes,
                duration_100ns,
            } if actual == request_id && actual_source == source_id => {
                Ok((encoded_bytes, duration_100ns, buffered))
            }
            WorkerMediaMessage::DecodeFailed {
                request_id: actual,
                error,
            } if actual == request_id => {
                Err(format!("media worker rejected adaptive append: {error}"))
            }
            _ => Err("media worker returned the wrong adaptive append response".into()),
        }
    }
}
