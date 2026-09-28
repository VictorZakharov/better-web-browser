//! One bounded FIFO per renderer for sensor activation, readings, and errors.

use crate::renderer_protocol::{DocumentId, SensorEvent, SensorUpdate};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

#[derive(Debug, PartialEq, Eq)]
pub enum SensorSinkError {
    Invalid(String),
    Full,
    Hidden,
    Disconnected,
}

#[derive(Clone, Default)]
pub struct SensorDeliveryGate(Arc<Mutex<DeliveryState>>);

#[derive(Default)]
struct DeliveryState {
    visible_tab: u64,
    generation: u64,
}

impl SensorDeliveryGate {
    pub fn set_visible_tab(&self, tab: u64) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.visible_tab != tab {
            state.generation = state.generation.wrapping_add(1);
            state.visible_tab = tab;
        }
    }

    pub fn clear_visible_tab(&self, tab: u64) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.visible_tab == tab {
            state.generation = state.generation.wrapping_add(1);
            state.visible_tab = 0;
        }
    }

    fn generation_for(&self, tab: u64) -> Option<u64> {
        let state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        (state.visible_tab == tab).then_some(state.generation)
    }
}

pub(crate) struct QueuedSensorUpdate {
    pub update: SensorUpdate,
    reading_gate: Option<(SensorDeliveryGate, u64, u64)>,
}

impl QueuedSensorUpdate {
    /// Control events settle JS state, but a reading queued before a tab
    /// hides must not be replayed when that tab becomes visible again.
    pub fn forward_if_current(
        &self,
        forward: impl FnOnce(&SensorUpdate) -> Result<(), String>,
    ) -> Result<bool, String> {
        let Some((gate, tab, generation)) = &self.reading_gate else {
            return forward(&self.update).map(|()| true);
        };
        // Hold this short lock across the nonblocking writer enqueue. A tab
        // switch's clear waits for any earlier forwarding to finish.
        let state = gate.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.visible_tab != *tab || state.generation != *generation {
            return Ok(false);
        }
        forward(&self.update).map(|()| true)
    }
}

#[derive(Clone)]
pub struct SensorUpdateSink {
    document: DocumentId,
    updates: mpsc::SyncSender<QueuedSensorUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::super::wake::BrokerWake,
    delivery: Option<(SensorDeliveryGate, u64)>,
}

impl SensorUpdateSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        updates: mpsc::SyncSender<QueuedSensorUpdate>,
        overflow: Arc<AtomicBool>,
        wake: super::super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            updates,
            overflow,
            wake,
            delivery: None,
        }
    }

    pub fn with_delivery_gate(mut self, gate: SensorDeliveryGate, tab: u64) -> Self {
        self.delivery = Some((gate, tab));
        self
    }

    /// Samples may be dropped under pressure; state and Promise completions may not.
    /// One FIFO preserves Activate-before-Reading. A saturated control update
    /// terminates the renderer instead of leaving promises pending.
    pub fn try_send(&self, update: SensorUpdate) -> Result<(), SensorSinkError> {
        update
            .validate()
            .map_err(|error| SensorSinkError::Invalid(error.to_string()))?;
        if update.document != self.document {
            return Err(SensorSinkError::Invalid(
                "sensor update document mismatch".into(),
            ));
        }
        let reading_gate = if matches!(update.event, SensorEvent::Reading(_)) {
            self.delivery
                .as_ref()
                .map(|(gate, tab)| {
                    gate.generation_for(*tab)
                        .map(|generation| (gate.clone(), *tab, generation))
                        .ok_or(SensorSinkError::Hidden)
                })
                .transpose()?
        } else {
            None
        };
        let queued = QueuedSensorUpdate {
            update,
            reading_gate,
        };
        let result = if matches!(queued.update.event, SensorEvent::Reading(_)) {
            match self.updates.try_send(queued) {
                Ok(()) => Ok(()),
                Err(mpsc::TrySendError::Full(_)) => Err(SensorSinkError::Full),
                Err(mpsc::TrySendError::Disconnected(_)) => Err(SensorSinkError::Disconnected),
            }
        } else {
            match self.updates.try_send(queued) {
                Ok(()) => Ok(()),
                Err(mpsc::TrySendError::Full(_)) => {
                    self.overflow.store(true, Ordering::Release);
                    Err(SensorSinkError::Full)
                }
                Err(mpsc::TrySendError::Disconnected(_)) => Err(SensorSinkError::Disconnected),
            }
        };
        self.wake.notify();
        result
    }
}
