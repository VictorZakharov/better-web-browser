//! Lossless, bounded control replies from browser services into the renderer.

use crate::renderer_protocol::{
    ClipboardUpdate, DocumentId, GeolocationUpdate, MediaDeviceUpdate, PermissionUpdate,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

#[derive(Clone)]
pub struct GeolocationUpdateSink {
    document: DocumentId,
    required: mpsc::SyncSender<GeolocationUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct MediaDeviceUpdateSink {
    document: DocumentId,
    required: mpsc::SyncSender<MediaDeviceUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct ClipboardUpdateSink {
    document: DocumentId,
    required: mpsc::SyncSender<ClipboardUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct PermissionUpdateSink {
    document: DocumentId,
    required: mpsc::SyncSender<PermissionUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
}

impl ClipboardUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        required: mpsc::SyncSender<ClipboardUpdate>,
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

    pub fn try_send(&self, update: ClipboardUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("clipboard update document mismatch".into());
        }
        match self.required.try_send(update) {
            Ok(()) => {
                self.wake.notify();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                Err("renderer clipboard control mailbox is full".into())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("renderer clipboard control mailbox is closed".into())
            }
        }
    }
}

impl PermissionUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        required: mpsc::SyncSender<PermissionUpdate>,
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

    pub fn try_send(&self, update: PermissionUpdate) -> Result<(), String> {
        if update.document != self.document || update.request_id == 0 {
            return Err("permission update identity mismatch".into());
        }
        match self.required.try_send(update) {
            Ok(()) => {
                self.wake.notify();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                Err("renderer permission control mailbox is full".into())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("renderer permission control mailbox is closed".into())
            }
        }
    }
}

impl GeolocationUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        required: mpsc::SyncSender<GeolocationUpdate>,
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

    /// Location callbacks must never block the UI thread or disappear silently.
    pub fn try_send(&self, update: GeolocationUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("geolocation update document mismatch".into());
        }
        match self.required.try_send(update) {
            Ok(()) => {
                self.wake.notify();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                Err("renderer geolocation control mailbox is full".into())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("renderer geolocation control mailbox is closed".into())
            }
        }
    }
}

impl MediaDeviceUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        required: mpsc::SyncSender<MediaDeviceUpdate>,
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

    pub fn try_send(&self, update: MediaDeviceUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("media-device update document mismatch".into());
        }
        match self.required.try_send(update) {
            Ok(()) => {
                self.wake.notify();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                Err("renderer media-device control mailbox is full".into())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("renderer media-device control mailbox is closed".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer_protocol::{PermissionName, PermissionState};

    #[test]
    fn permission_reply_overflow_is_explicit_and_never_silent() {
        let document = DocumentId::new(7).unwrap();
        let (sender, _receiver) = mpsc::sync_channel(1);
        let overflow = Arc::new(AtomicBool::new(false));
        let sink = PermissionUpdateSink::new(
            document,
            sender,
            Arc::clone(&overflow),
            super::super::super::wake::BrokerWake::default(),
        );
        let update = PermissionUpdate {
            document,
            request_id: 1,
            name: PermissionName::Geolocation,
            state: PermissionState::Prompt,
            rejected: false,
        };
        assert!(sink.try_send(update.clone()).is_ok());
        assert!(sink.try_send(update).is_err());
        assert!(overflow.load(Ordering::Acquire));
    }
}
