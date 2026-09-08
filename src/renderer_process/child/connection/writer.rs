//! The document and video producer share a sequenced pipe, never a JavaScript realm.
use super::*;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub(super) struct SharedWriter(Arc<Mutex<FrameWriter<File>>>);

impl SharedWriter {
    pub(super) fn new(writer: FrameWriter<File>) -> Self {
        Self(Arc::new(Mutex::new(writer)))
    }

    pub(super) fn lock(&self) -> Result<MutexGuard<'_, FrameWriter<File>>, ProtocolError> {
        self.0
            .lock()
            .map_err(|_| ProtocolError::InvalidPayload("renderer writer poisoned"))
    }

    pub(super) fn send_renderer(&self, message: &RendererMessage) -> Result<(), ProtocolError> {
        self.lock()?.send_renderer(message)
    }
}
