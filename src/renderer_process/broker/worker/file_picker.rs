//! Forwards bounded selections only while their document is still active.

use super::{Broker, BrowserMessage};
use std::sync::atomic::Ordering;
use std::sync::mpsc;

impl Broker {
    pub(super) fn process_file_picker_updates(&mut self) {
        if self
            .resources()
            .file_picker_overflow
            .load(Ordering::Acquire)
        {
            self.protocol_failure("renderer file picker mailbox overflow".into());
            return;
        }
        for _ in 0..crate::limits::MAX_QUEUED_FETCH_STREAM_CHUNKS {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let update = match self.resources().file_picker_updates.try_recv() {
                Ok(update) => update,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            };
            if self.active_document != Some(update.document()) {
                continue;
            }
            let result = update
                .validate()
                .map_err(|error| error.to_string())
                .and_then(|_| {
                    self.writer()
                        .send_browser(&BrowserMessage::FilePickerUpdate(update))
                        .map_err(|error| error.to_string())
                });
            if let Err(error) = result {
                self.protocol_failure(error);
                break;
            }
        }
    }
}
