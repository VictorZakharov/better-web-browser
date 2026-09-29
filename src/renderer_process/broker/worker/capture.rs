use super::*;
use std::sync::atomic::Ordering;

impl Broker {
    pub(super) fn process_media_capture_updates(&mut self) {
        if self
            .resources()
            .media_capture_overflow
            .load(Ordering::Acquire)
        {
            self.protocol_failure("renderer capture control mailbox overflow".into());
            return;
        }
        for _ in 0..64 {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let update = match self.resources().media_capture_updates.try_recv() {
                Ok(update) => update,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            };
            if self.active_document != Some(update.document) {
                continue;
            }
            if let Err(error) = self
                .writer()
                .send_browser(&BrowserMessage::MediaCaptureUpdate(update))
            {
                self.protocol_failure(error.to_string());
                break;
            }
        }
    }

    pub(super) fn process_media_capture_frames(&mut self) {
        for _ in 0..9 {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let frame = self
                .resources()
                .media_capture_frames
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take_one();
            let Some(frame) = frame else { break };
            if self.active_document != Some(frame.document) {
                continue;
            }
            if let Err(error) = self
                .writer()
                .send_browser(&BrowserMessage::MediaCaptureFrame(frame))
            {
                self.protocol_failure(error.to_string());
                break;
            }
        }
    }
}
