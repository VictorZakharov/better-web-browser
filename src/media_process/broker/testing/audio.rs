//! Test-only complete-source audio path through the contained worker, with no video pipe.

use super::*;

impl MediaSession {
    #[doc(hidden)]
    pub fn decode_owned_audio_fixture(
        &mut self,
        bytes: &[u8],
    ) -> Result<(u64, MediaDecodeReport), String> {
        self.require_test_mode()?;
        if bytes.is_empty() || bytes.len() as u64 > self.limits.max_encoded_queue_bytes {
            return Err("owned audio fixture exceeds worker limits".into());
        }
        let request_id = self.allocate_request()?;
        let source_id = self.next_source;
        self.next_source = source_id
            .checked_add(1)
            .ok_or_else(|| "media source identity exhausted".to_string())?;
        let frame_id = self.next_frame;
        self.next_frame = frame_id
            .checked_add(1)
            .ok_or_else(|| "media frame identity exhausted".to_string())?;
        let source = MediaSourceId::new(source_id).map_err(|error| error.to_string())?;
        self.send(
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
                frame_id,
                encoded_length: bytes.len() as u64,
            },
            "request audio-only decode",
        )?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = MediaSessionId::new(self.session_id).map_err(|error| error.to_string())?;
        let nonce = self.nonce;
        let (response, sent) = std::thread::scope(|scope| {
            let sender = scope.spawn(move || {
                MediaDataWriter::new(output, session, nonce).send_source(source, bytes)
            });
            let response = self.receive("decode audio-only source", self.command_timeout);
            let sent = sender
                .join()
                .map_err(|_| "media data writer panicked".to_string())?
                .map_err(|error| format!("deliver owned audio bytes: {error}"));
            Ok::<_, String>((response, sent))
        })?;
        sent?;
        match response? {
            WorkerMediaMessage::Decoded {
                request_id: actual,
                report,
                frame: None,
            } if actual == request_id => {
                report
                    .validate(self.limits)
                    .map_err(|error| format!("invalid audio-only report: {error}"))?;
                Ok((source_id, report))
            }
            WorkerMediaMessage::DecodeFailed {
                request_id: actual,
                error,
            } if actual == request_id => {
                Err(format!("media worker rejected audio-only decode: {error}"))
            }
            _ => {
                self.protocol_failure("media worker returned the wrong audio-only decode response")
            }
        }
    }

    #[doc(hidden)]
    pub fn seek_owned_fixture_playback(
        &mut self,
        source_id: u64,
        position_100ns: u64,
    ) -> Result<MediaPlaybackState, String> {
        self.require_test_mode()?;
        self.send(
            BrowserMediaMessage::SeekPlayback {
                source_id,
                position_100ns,
            },
            "seek owned audio fixture",
        )?;
        self.receive_owned_fixture_state(source_id, "seek owned audio fixture")
    }
}
