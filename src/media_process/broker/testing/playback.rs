use super::super::{DecodedMediaFrame, MEDIA_EXIT_PROTOCOL, MediaSession, OwnedMediaPlayback};
use crate::media_frame_protocol::{MediaFrameReader as DecodedFrameReader, nv12_to_bgra};
use crate::media_protocol::{BrowserMediaMessage, MediaSessionId, WorkerMediaMessage};

impl MediaSession {
    /// Pulls a bounded sequence through the production acknowledgement path. Remote bytes remain
    /// unavailable here: this adapter accepts only browser-owned fixtures in explicit test mode.
    #[doc(hidden)]
    pub fn decode_owned_fixture_frames(
        &mut self,
        bytes: &[u8],
        maximum_frames: usize,
    ) -> Result<OwnedMediaPlayback, String> {
        self.require_test_mode()?;
        if maximum_frames == 0 {
            return Err("owned media playback requires at least one frame".into());
        }
        let first = self.decode_owned_fixture_frame(bytes)?;
        let report = first.report;
        let source_id = first.frame.metadata.source_id;
        let target = maximum_frames.min(report.video_samples as usize);
        let mut frames = Vec::with_capacity(target);
        frames.push(first.frame);

        while frames.len() < target {
            let frame_id = self.next_frame;
            self.next_frame = frame_id
                .checked_add(1)
                .ok_or_else(|| "media frame identity exhausted".to_string())?;
            self.send(
                BrowserMediaMessage::RequestFrame {
                    source_id,
                    frame_id,
                },
                "request decoded media frame",
            )?;
            let frame_input = self
                .frame_input
                .try_clone()
                .map_err(|error| format!("clone media frame pipe: {error}"))?;
            let session =
                MediaSessionId::new(self.session_id).map_err(|error| error.to_string())?;
            let (response, received_frame) = std::thread::scope(|scope| {
                let nonce = self.nonce;
                let receiver = scope.spawn(move || {
                    DecodedFrameReader::new(frame_input, session, nonce)
                        .read_frame(source_id, frame_id)
                });
                let response = self.receive("decode next frame", self.command_timeout);
                let frame = receiver
                    .join()
                    .map_err(|_| "media frame reader panicked".to_string())
                    .and_then(|result| result.map_err(|error| error.to_string()));
                (response, frame)
            });
            let packet = match received_frame {
                Ok(frame) => frame,
                Err(error) => {
                    self.mark_exited(
                        format!("could not receive decoded media frame: {error}"),
                        MEDIA_EXIT_PROTOCOL,
                    );
                    return Err(self.exit_reason.clone().unwrap_or_default());
                }
            };
            let metadata = match response? {
                WorkerMediaMessage::FrameReady { frame }
                    if frame.source_id == source_id && frame.frame_id == frame_id =>
                {
                    frame
                }
                _ => {
                    return self.protocol_failure("media worker returned the wrong frame response");
                }
            };
            if packet.metadata != metadata {
                return self
                    .protocol_failure("media frame metadata disagreed with control response");
            }
            let converted = nv12_to_bgra(metadata, &packet.nv12)
                .map_err(|error| format!("convert decoded NV12 frame: {error}"))?;
            self.send(
                BrowserMediaMessage::AcknowledgeFrame {
                    source_id,
                    frame_id,
                },
                "acknowledge decoded media frame",
            )?;
            match self.receive("frame acknowledgement", self.command_timeout)? {
                WorkerMediaMessage::FrameAcknowledged {
                    source_id: actual_source,
                    frame_id: actual_frame,
                } if actual_source == source_id && actual_frame == frame_id => {}
                _ => {
                    return self
                        .protocol_failure("media worker returned a stale frame acknowledgement");
                }
            }
            frames.push(DecodedMediaFrame {
                metadata,
                nv12: packet.nv12,
                bgra: converted.bgra,
            });
        }
        Ok(OwnedMediaPlayback { report, frames })
    }
}
