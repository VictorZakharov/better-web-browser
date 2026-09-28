//! Lossless, bounded control replies from browser services into the renderer.

use crate::renderer_protocol::{DocumentId, GeolocationUpdate, MediaDeviceUpdate};
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
