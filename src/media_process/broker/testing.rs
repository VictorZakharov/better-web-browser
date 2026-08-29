use super::{MEDIA_EXIT_PROTOCOL, MEDIA_EXIT_TIMEOUT, MediaSession};
use crate::media_data_protocol::{MediaDataWriter, MediaSourceId};
use crate::media_protocol::{
    BrowserMediaMessage, MediaDecodeReport, MediaRestrictionReport, MediaSessionId,
    MediaTestCommand, WorkerMediaMessage,
};
use std::sync::mpsc;

impl MediaSession {
    /// Exercises production media data framing and decode with browser-owned test bytes.
    /// Remote loading remains closed until a contained network service can feed this pipe.
    #[doc(hidden)]
    pub fn decode_owned_fixture(&mut self, bytes: &[u8]) -> Result<MediaDecodeReport, String> {
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
        self.send(
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
                encoded_length: bytes.len() as u64,
            },
            "request media decode",
        )?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = MediaSessionId::new(self.session_id).map_err(|error| error.to_string())?;
        let (response, sent) = std::thread::scope(|scope| {
            let nonce = self.nonce;
            let sender = scope.spawn(move || {
                MediaDataWriter::new(output, session, nonce).send_source(source, bytes)
            });
            let response = self.receive("decode", self.command_timeout);
            let sent = sender
                .join()
                .map_err(|_| "media data writer panicked".to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            (response, sent)
        });
        if let Err(error) = sent {
            self.mark_exited(
                format!("could not deliver encoded media: {error}"),
                MEDIA_EXIT_PROTOCOL,
            );
            return Err(self.exit_reason.clone().unwrap_or_default());
        }
        match response? {
            WorkerMediaMessage::Decoded {
                request_id: actual,
                report,
            } if actual == request_id => {
                if let Err(error) = report.validate(self.limits) {
                    return self.protocol_failure(&format!("invalid media decode report: {error}"));
                }
                Ok(report)
            }
            _ => self.protocol_failure("media worker returned the wrong decode response"),
        }
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
        self.send(
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
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
        if let Ok(message) = self.receive("oversized data test", self.command_timeout) {
            return self.protocol_failure(&format!(
                "oversized media data unexpectedly returned {message:?}"
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
        if matches!(command, MediaTestCommand::ProbeRestrictions { .. }) {
            return Err("use probe_restrictions for restriction probes".into());
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
