//! Nonblocking, document-scoped file-selection updates from browser UI.

use crate::renderer_protocol::{DocumentId, FilePickerUpdate};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

#[derive(Clone)]
pub struct FilePickerUpdateSink {
    document: DocumentId,
    required: mpsc::SyncSender<FilePickerUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
}

impl FilePickerUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        required: mpsc::SyncSender<FilePickerUpdate>,
        overflow: Arc<AtomicBool>,
        wake: super::super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            required,
            overflow,
            wake,
        }
    }

    pub fn try_send(&self, update: FilePickerUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document() != self.document {
            return Err("file picker update document mismatch".into());
        }
        match self.required.try_send(update) {
            Ok(()) => {
                self.wake.notify();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                // A partial selection must never be mistaken for a complete one.
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                Err("renderer file picker mailbox is full".into())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("renderer file picker mailbox is closed".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_scope_and_overflow_fail_closed() {
        let document = DocumentId::new(3).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        let overflow = Arc::new(AtomicBool::new(false));
        let sink =
            FilePickerUpdateSink::new(document, sender, Arc::clone(&overflow), Default::default());
        let update = FilePickerUpdate::Canceled {
            document,
            request_id: 9,
        };
        sink.try_send(update.clone()).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), update);
        assert!(
            sink.try_send(FilePickerUpdate::Failed {
                document: DocumentId::new(4).unwrap(),
                request_id: 9,
            })
            .is_err()
        );
        sink.try_send(update.clone()).unwrap();
        assert!(sink.try_send(update).is_err());
        assert!(overflow.load(Ordering::Acquire));
    }
}
