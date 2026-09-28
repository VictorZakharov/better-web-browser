//! One bounded FIFO per renderer for sensor activation, readings, and errors.

use crate::renderer_protocol::{DocumentId, SensorEvent, SensorUpdate};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

#[derive(Clone)]
pub struct SensorUpdateSink {
    document: DocumentId,
    updates: mpsc::SyncSender<SensorUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
}

impl SensorUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        updates: mpsc::SyncSender<SensorUpdate>,
        overflow: Arc<AtomicBool>,
        wake: super::super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            updates,
            overflow,
            wake,
        }
    }

    /// Samples may be dropped under pressure; state and Promise completions may not.
    /// One FIFO preserves Activate-before-Reading. A saturated control update
    /// terminates the renderer instead of leaving promises pending.
    pub fn try_send(&self, update: SensorUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("sensor update document mismatch".into());
        }
        let result = if matches!(update.event, SensorEvent::Reading(_)) {
            self.updates
                .try_send(update)
                .map_err(|error| format!("renderer sensor reading mailbox unavailable: {error}"))
        } else {
            match self.updates.try_send(update) {
                Ok(()) => Ok(()),
                Err(mpsc::TrySendError::Full(_)) => {
                    self.overflow.store(true, Ordering::Release);
                    Err("renderer sensor control mailbox is full".into())
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    Err("renderer sensor control mailbox is closed".into())
                }
            }
        };
        self.wake.notify();
        result
    }
}
