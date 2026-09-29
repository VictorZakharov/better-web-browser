//! Capture updates settle script promises; sample delivery is intentionally separate.

use super::super::{AdvanceResult, DocumentRuntime};
use crate::engine::DecodedImage;
use crate::media_frame_protocol::{MediaPixelFormat, MediaVideoFrameMetadata, nv12_to_bgra};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::{
    MediaCaptureFrame, MediaCaptureFrameKind, MediaCaptureUpdate, VideoFrameIdentity,
};
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn deliver_media_capture_update(
        &mut self,
        update: MediaCaptureUpdate,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if update.document != self.id {
            return Ok(None);
        }
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        let outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.deliver_media_capture_update(update))
            .unwrap_or_default();
        self.complete_network_script_outcome(
            outcome,
            true,
            previous_timer_micros,
            started,
            connection,
        )
    }

    pub(in crate::renderer_process::child) fn deliver_media_capture_frame(
        &mut self,
        frame: MediaCaptureFrame,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if frame.document != self.id {
            return Ok(None);
        }
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        if frame.kind == MediaCaptureFrameKind::AudioPcm16 {
            let outcome = self
                .script_runtime
                .as_mut()
                .map(|runtime| runtime.deliver_media_capture_audio_frame(frame))
                .unwrap_or_default();
            return self.complete_network_script_outcome(
                outcome,
                false,
                previous_timer_micros,
                started,
                connection,
            );
        }
        let mut outcome = self
            .script_runtime
            .as_mut()
            .filter(|_| frame.kind == MediaCaptureFrameKind::VideoNv12)
            .map(|runtime| {
                runtime.deliver_media_capture_frame_info(
                    frame.request_id,
                    frame.width_or_rate,
                    frame.height_or_frames,
                    frame.timestamp_100ns,
                )
            })
            .unwrap_or_default();
        // Frame events run author script. A loadeddata/timeupdate listener may detach
        // srcObject, stop the track, or disable it before this sample is painted.
        // Read the live bindings after that callback, never from a stale snapshot.
        let nodes = self
            .script_runtime
            .as_ref()
            .map(|runtime| runtime.capture_video_nodes(frame.request_id))
            .unwrap_or_default();
        if frame.kind != MediaCaptureFrameKind::VideoNv12 || nodes.is_empty() {
            return self.complete_network_script_outcome(
                outcome,
                false,
                previous_timer_micros,
                started,
                connection,
            );
        }

        // The capture protocol already validated allocation shape and 2 MiB budget. The media
        // converter's timestamp bound is for finite files; live timing stays on the capture frame.
        let metadata = MediaVideoFrameMetadata {
            source_id: frame.request_id,
            frame_id: frame.sequence,
            timestamp_100ns: 0,
            duration_100ns: 333_333,
            width: frame.width_or_rate,
            height: frame.height_or_frames,
            stride: frame.stride_or_channels,
            format: MediaPixelFormat::Nv12,
            data_length: frame.bytes.len() as u64,
        };
        let decoded = nv12_to_bgra(metadata, &frame.bytes)
            .map_err(|error| format!("capture NV12 conversion failed: {error}"))?;
        let black = nodes.iter().any(|(_, enabled)| !enabled).then(|| {
            let mut pixels = vec![0_u8; decoded.bgra.len()];
            for alpha in pixels[3..].iter_mut().step_by(4) {
                *alpha = 255;
            }
            DecodedImage {
                width: decoded.width,
                height: decoded.height,
                bgra: pixels.into(),
            }
        });
        let mut needs_layout = false;
        let mut direct = Vec::new();
        for (node, enabled) in nodes {
            let Some(target) = self.page.dom.find_node(node) else {
                continue;
            };
            if target.tag_name() != Some("video") {
                continue;
            }
            let identity = VideoFrameIdentity {
                document: self.id,
                revision: self.revision.max(1),
                frame: frame.sequence,
                node: node.to_wire(),
                width: decoded.width,
                height: decoded.height,
            };
            let previous = self.page.images.get(&identity.image_key());
            let first_or_resized = previous
                .is_none_or(|image| image.width != decoded.width || image.height != decoded.height);
            let image = if enabled {
                decoded.clone()
            } else {
                black.as_ref().unwrap().clone()
            };
            match self.page.install_media_frame(node, image.clone()) {
                Ok(key) => {
                    let selected_changed = self.page.select_media_video_track(node, true);
                    if first_or_resized || selected_changed {
                        self.sent_images.remove(&key);
                        needs_layout = true;
                    }
                    if !first_or_resized {
                        direct.push((node, image.bgra));
                    }
                }
                Err(error) => outcome
                    .errors
                    .push(format!("capture frame presentation: {error}")),
            }
        }
        outcome.render_requested |= needs_layout;
        let result = self.complete_network_script_outcome(
            outcome,
            false,
            previous_timer_micros,
            started,
            connection,
        )?;
        // Pixel-only updates do not need a full display-list re-layout. Reuse the existing
        // revision-bound video frame path; any script-triggered presentation is ordered first.
        if self.revision != 0 {
            for (node, pixels) in direct {
                connection.send_capture_video_frame(
                    VideoFrameIdentity {
                        document: self.id,
                        revision: self.revision,
                        frame: frame.sequence,
                        node: node.to_wire(),
                        width: decoded.width,
                        height: decoded.height,
                    },
                    &pixels,
                )?;
            }
        }
        Ok(result)
    }
}
