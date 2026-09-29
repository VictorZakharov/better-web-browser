//! Document-scoped browser capture requests and updates.

use super::*;
use crate::renderer_protocol::{
    MediaCaptureFrame, MediaCaptureRequest, MediaCaptureUpdate, VideoFrameChunk, VideoFrameIdentity,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_capture_video_frame(
        &mut self,
        identity: VideoFrameIdentity,
        bgra: &[u8],
    ) -> Result<(), String> {
        if bgra.len() != identity.byte_length().map_err(|error| error.to_string())? {
            return Err("capture video frame size mismatch".into());
        }
        let mut writer = self.writer.lock().map_err(|error| error.to_string())?;
        for (index, bytes) in bgra.chunks(1024 * 1024).enumerate() {
            writer
                .send_renderer(&RendererMessage::VideoFrame(VideoFrameChunk {
                    identity: identity.clone(),
                    offset: (index * 1024 * 1024) as u32,
                    bytes: bytes.to_vec(),
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn send_media_capture_request(
        &mut self,
        request: MediaCaptureRequest,
    ) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        self.writer
            .send_renderer(&RendererMessage::MediaCaptureRequest(request))
            .map_err(|error| error.to_string())
    }

    pub(super) fn deliver_media_capture_update(
        &mut self,
        update: MediaCaptureUpdate,
    ) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != update.document {
            self.document = Some(runtime);
            return Ok(());
        }
        let document = update.document;
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_media_capture_update(update, self)
        }));
        match result {
            Ok(Ok(Some(update))) => self.send_document_update(update)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => return self.send_document_failure(document, error),
            Err(payload) => {
                return self.send_document_failure(document, super::runtime::panic_detail(payload));
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    pub(super) fn deliver_media_capture_frame(
        &mut self,
        frame: MediaCaptureFrame,
    ) -> Result<(), String> {
        frame.validate().map_err(|error| error.to_string())?;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != frame.document {
            self.document = Some(runtime);
            return Ok(());
        }
        let document = frame.document;
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_media_capture_frame(frame, self)
        }));
        match result {
            Ok(Ok(Some(update))) => self.send_document_update(update)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => return self.send_document_failure(document, error),
            Err(payload) => {
                return self.send_document_failure(document, super::runtime::panic_detail(payload));
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }
}
