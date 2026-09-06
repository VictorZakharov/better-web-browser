//! Pixel delivery never acknowledges work on the JavaScript/document thread.
use super::*;

impl Broker {
    pub(super) fn process_video_chunk(&mut self, chunk: crate::renderer_protocol::VideoFrameChunk) {
        // Assemble even retired-document chunks: navigation may occur halfway through a frame.
        // Its complete result is discarded, without resetting any document watchdog deadline.
        match self.incoming_video.push(chunk) {
            Ok(Some(update)) if Some(update.identity.document) == self.active_document => {
                if let Err(error) = self.emit_event(RendererEvent::VideoFrame(Box::new(update))) {
                    self.protocol_failure(error.to_string());
                }
            }
            Ok(_) => {}
            Err(error) => self.protocol_failure(error.to_string()),
        }
    }
}
