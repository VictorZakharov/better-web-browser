use super::{
    DecodedMediaFrame, MEDIA_EXIT_DECODE, MEDIA_EXIT_PROTOCOL, MEDIA_EXIT_TIMEOUT, MediaSession,
    OwnedMediaDecode,
};
use crate::media_data_protocol::{MediaDataWriter, MediaSourceId};
use crate::media_frame_protocol::{MediaFrameReader as DecodedFrameReader, nv12_to_bgra};
use crate::media_protocol::{
    BrowserMediaMessage, MediaDecodeReport, MediaPlaybackState, MediaRestrictionReport,
    MediaSessionId, MediaTestCommand, WorkerMediaMessage,
};
use std::sync::mpsc;

mod playback;

impl MediaSession {
    #[doc(hidden)]
    pub fn set_owned_fixture_playback(
        &mut self,
        source_id: u64,
        playing: bool,
        volume_millis: u16,
    ) -> Result<MediaPlaybackState, String> {
        self.require_test_mode()?;
        self.send(
            BrowserMediaMessage::SetPlayback {
                source_id,
                playing,
                volume_millis,
            },
            "set owned fixture playback",
        )?;
        self.receive_owned_fixture_state(source_id, "set owned fixture playback")
    }

    #[doc(hidden)]
    pub fn owned_fixture_playback_state(
        &mut self,
        source_id: u64,
    ) -> Result<MediaPlaybackState, String> {
        self.require_test_mode()?;
        self.send(
            BrowserMediaMessage::PlaybackState { source_id },
            "query owned fixture playback",
        )?;
        self.receive_owned_fixture_state(source_id, "query owned fixture playback")
    }

    fn receive_owned_fixture_state(
        &mut self,
        source_id: u64,
        operation: &str,
    ) -> Result<MediaPlaybackState, String> {
        match self.receive(operation, self.command_timeout)? {
            WorkerMediaMessage::PlaybackState(state) if state.source_id == source_id => {
                state
                    .validate()
                    .map_err(|error| format!("invalid owned fixture playback state: {error}"))?;
                Ok(state)
            }
            _ => self.protocol_failure("media worker returned stale playback state"),
        }
    }

    /// Exercises production media data framing and decode with browser-owned test bytes.
    /// Remote loading remains closed until a contained network service can feed this pipe.
    #[doc(hidden)]
    pub fn decode_owned_fixture(&mut self, bytes: &[u8]) -> Result<MediaDecodeReport, String> {
        self.decode_owned_fixture_frame(bytes)
            .map(|decode| decode.report)
    }

    /// Exercises the complete encoded-input, decoded-frame, and acknowledgement path.
    #[doc(hidden)]
    pub fn decode_owned_fixture_frame(&mut self, bytes: &[u8]) -> Result<OwnedMediaDecode, String> {
        self.require_test_mode()?;
        if bytes.is_empty() || bytes.len() as u64 > self.limits.max_encoded_queue_bytes {
            return Err("owned media fixture exceeds worker limits".into());
        }
        let request_id = self.allocate_request()?;
        let source_id = self.next_source;
        self.next_source = source_id
            .checked_add(1)
            .ok_or_else(|| "media source identity exhausted".to_string())?;
        let source = MediaSourceId::new(source_id).map_err(|error| error.to_string())?;
        let frame_id = self.next_frame;
        self.next_frame = frame_id
            .checked_add(1)
            .ok_or_else(|| "media frame identity exhausted".to_string())?;
        self.send(
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
                frame_id,
                encoded_length: bytes.len() as u64,
            },
            "request media decode",
        )?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = MediaSessionId::new(self.session_id).map_err(|error| error.to_string())?;
        let frame_input = self
            .frame_input
            .try_clone()
            .map_err(|error| format!("clone media frame pipe: {error}"))?;
        let (response, sent, received_frame) = std::thread::scope(|scope| {
            let nonce = self.nonce;
            let sender = scope.spawn(move || {
                MediaDataWriter::new(output, session, nonce).send_source(source, bytes)
            });
            let frame_receiver = scope.spawn(move || {
                DecodedFrameReader::new(frame_input, session, nonce).read_frame(source_id, frame_id)
            });
            let response = self.receive("decode", self.command_timeout);
            let failure = match &response {
                Ok(WorkerMediaMessage::Decoded { .. }) => None,
                Ok(WorkerMediaMessage::DecodeFailed { error, .. }) => {
                    Some(format!("media worker rejected decode: {error}"))
                }
                Ok(_) => Some("media worker returned the wrong decode response".into()),
                Err(_) => None,
            };
            if let Some(reason) = failure {
                // A failed decode has no frame packet. Close the contained worker before joining
                // the concurrent frame reader so the pipe reaches EOF instead of waiting forever.
                self.mark_exited(reason, MEDIA_EXIT_DECODE);
            }
            let sent = sender
                .join()
                .map_err(|_| "media data writer panicked".to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let received_frame = frame_receiver
                .join()
                .map_err(|_| "media frame reader panicked".to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            (response, sent, received_frame)
        });
        if let Err(error) = sent {
            self.mark_exited(
                format!("could not deliver encoded media: {error}"),
                MEDIA_EXIT_PROTOCOL,
            );
            return Err(self.exit_reason.clone().unwrap_or_default());
        }
        let (report, metadata) = match response {
            Err(error) => return Err(error),
            Ok(WorkerMediaMessage::Decoded {
                request_id: actual,
                report,
                frame,
            }) if actual == request_id => {
                if let Err(error) = report.validate(self.limits) {
                    return self.protocol_failure(&format!("invalid media decode report: {error}"));
                }
                (report, frame)
            }
            Ok(WorkerMediaMessage::DecodeFailed { .. }) | Ok(_) => {
                return Err(self.exit_reason.clone().unwrap_or_default());
            }
        };
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
        if packet.metadata != metadata {
            return self.protocol_failure("media frame metadata disagreed with control response");
        }
        let converted = match nv12_to_bgra(metadata, &packet.nv12) {
            Ok(converted) => converted,
            Err(error) => {
                return self.protocol_failure(&format!("convert decoded NV12 frame: {error}"));
            }
        };
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
        Ok(OwnedMediaDecode {
            report,
            frame: DecodedMediaFrame {
                metadata,
                nv12: packet.nv12,
                bgra: converted.bgra,
            },
        })
    }

    /// Sends a deliberately oversized data frame after a valid bounded control declaration.
    #[doc(hidden)]
    pub fn inject_oversized_source(&mut self) -> Result<(), String> {
        self.require_test_mode()?;
        let request_id = self.allocate_request()?;
        let source_id = self.next_source;
        self.next_source = source_id
            .checked_add(1)
            .ok_or_else(|| "media source identity exhausted".to_string())?;
        let source = MediaSourceId::new(source_id).map_err(|error| error.to_string())?;
        let frame_id = self.next_frame;
        self.next_frame = frame_id
            .checked_add(1)
            .ok_or_else(|| "media frame identity exhausted".to_string())?;
        self.send(
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
                frame_id,
                encoded_length: 1,
            },
            "request oversized media test",
        )?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = MediaSessionId::new(self.session_id).map_err(|error| error.to_string())?;
        MediaDataWriter::new(output, session, self.nonce)
            .send_oversized_chunk_for_test(source)
            .map_err(|error| format!("send oversized media test: {error}"))?;
        match self.receive("oversized data test", self.command_timeout) {
            Ok(WorkerMediaMessage::DecodeFailed {
                request_id: actual,
                error,
            }) if actual == request_id => {
                self.mark_exited(
                    format!("media worker rejected oversized data framing: {error}"),
                    MEDIA_EXIT_PROTOCOL,
                );
            }
            Ok(message) => {
                return self.protocol_failure(&format!(
                    "oversized media data unexpectedly returned {message:?}"
                ));
            }
            Err(_) => {}
        }
        Ok(())
    }

    /// Proves that an acknowledgement without a pending frame terminates only this worker.
    #[doc(hidden)]
    pub fn inject_stale_frame_acknowledgement(&mut self) -> Result<(), String> {
        self.require_test_mode()?;
        self.send(
            BrowserMediaMessage::AcknowledgeFrame {
                source_id: 1,
                frame_id: 1,
            },
            "inject stale media frame acknowledgement",
        )?;
        if let Ok(message) = self.receive("stale frame acknowledgement", self.command_timeout) {
            return self.protocol_failure(&format!(
                "stale media frame acknowledgement unexpectedly returned {message:?}"
            ));
        }
        Ok(())
    }

    /// Exercises decoded-frame pipe rejection while also observing the worker's control exit.
    #[doc(hidden)]
    pub fn inject_frame_failure(&mut self, command: MediaTestCommand) -> Result<(), String> {
        self.require_test_mode()?;
        if !matches!(
            command,
            MediaTestCommand::WriteMalformedDecodedFrame
                | MediaTestCommand::WriteTruncatedDecodedFrame
                | MediaTestCommand::WriteOversizedDecodedFrame
        ) {
            return Err("use inject_frame_failure only for decoded-frame faults".into());
        }
        let frame_input = self
            .frame_input
            .try_clone()
            .map_err(|error| format!("clone media frame pipe: {error}"))?;
        let session = MediaSessionId::new(self.session_id).map_err(|error| error.to_string())?;
        self.send(
            BrowserMediaMessage::Test(command),
            "frame failure injection",
        )?;
        let (response, frame) = std::thread::scope(|scope| {
            let nonce = self.nonce;
            let receiver = scope.spawn(move || {
                DecodedFrameReader::new(frame_input, session, nonce).read_frame(1, 1)
            });
            let response = self.receive("frame failure injection", self.command_timeout);
            let frame = receiver
                .join()
                .map_err(|_| "media frame reader panicked".to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            (response, frame)
        });
        if let Ok(message) = response {
            return self.protocol_failure(&format!(
                "frame failure injection unexpectedly returned {message:?}"
            ));
        }
        if let Ok(frame) = frame {
            return self.protocol_failure(&format!(
                "invalid decoded frame was unexpectedly accepted: {:?}",
                frame.metadata
            ));
        }
        Ok(())
    }

    #[doc(hidden)]
    pub fn probe_restrictions(
        &mut self,
        loopback_port: u16,
    ) -> Result<MediaRestrictionReport, String> {
        self.require_test_mode()?;
        self.send(
            BrowserMediaMessage::Test(MediaTestCommand::ProbeRestrictions { loopback_port }),
            "restriction probe",
        )?;
        match self.receive("restriction probe", self.command_timeout)? {
            WorkerMediaMessage::Restrictions(report) => Ok(report),
            _ => self.protocol_failure("media worker returned the wrong restriction response"),
        }
    }

    #[doc(hidden)]
    pub fn inject_failure(&mut self, command: MediaTestCommand) -> Result<(), String> {
        self.require_test_mode()?;
        if matches!(
            command,
            MediaTestCommand::ProbeRestrictions { .. }
                | MediaTestCommand::WriteMalformedDecodedFrame
                | MediaTestCommand::WriteTruncatedDecodedFrame
                | MediaTestCommand::WriteOversizedDecodedFrame
        ) {
            return Err("use the specialized media test method for this command".into());
        }
        self.send(BrowserMediaMessage::Test(command), "failure injection")?;
        match self.incoming.recv_timeout(self.command_timeout) {
            Ok(Ok(message)) => self.protocol_failure(&format!(
                "failure injection unexpectedly returned {message:?}"
            )),
            Ok(Err(error)) => {
                self.mark_exited(
                    format!("media IPC failed after injected fault: {error}"),
                    MEDIA_EXIT_PROTOCOL,
                );
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.mark_exited(
                    "media worker exited after injected fault".into(),
                    MEDIA_EXIT_PROTOCOL,
                );
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Timeout) if command == MediaTestCommand::Hang => {
                self.mark_exited(
                    "media worker exceeded its command timeout".into(),
                    MEDIA_EXIT_TIMEOUT,
                );
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.mark_exited(
                    "media worker did not surface its injected failure".into(),
                    MEDIA_EXIT_TIMEOUT,
                );
                Err(self.exit_reason.clone().unwrap_or_default())
            }
        }
    }

    fn require_test_mode(&self) -> Result<(), String> {
        self.test_mode
            .then_some(())
            .ok_or_else(|| "media test command denied outside test mode".into())
    }
}
