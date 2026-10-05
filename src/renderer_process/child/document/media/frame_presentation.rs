//! Document-side media events and snapshots; video cadence belongs to the independent producer.
use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child::document) fn install_media_decode(
        &mut self,
        node: NodeId,
        decode: RendererMediaDecode,
        mime_type: String,
        origin_clean: bool,
    ) -> Result<(), String> {
        if let Some(runtime) = self.script_runtime.as_mut() {
            if let Some(previous) = self.media.as_ref() {
                runtime.clear_media_image(previous.node);
            }
            runtime.clear_media_image(node);
        }
        if self.page.select_media_video_track(node, true) {
            self.rendering.dirty = true;
        }
        let report = decode.report;
        let (clock, frame_end, width, height, frames_submitted) = if let Some(frame) = decode.frame
        {
            let metadata = frame.metadata;
            let image = DecodedImage {
                width: metadata.width,
                height: metadata.height,
                bgra: frame.bgra,
            };
            let key = self.page.install_media_frame(node, image.clone())?;
            if let Some(runtime) = self.script_runtime.as_mut() {
                runtime.set_media_image(node, image, origin_clean);
            }
            self.sent_images.remove(&key);
            (
                metadata.timestamp_100ns.max(0) as u64,
                frame_end(metadata),
                metadata.width,
                metadata.height,
                1,
            )
        } else {
            (
                report.buffered.audio_start_100ns.max(0) as u64,
                report.duration_100ns,
                0,
                0,
                0,
            )
        };
        self.media = Some(MediaPlayback {
            node,
            source_id: decode.source_id,
            clock_100ns: clock,
            frame_end_100ns: frame_end,
            duration_100ns: report.duration_100ns,
            buffered: report.buffered,
            playing: false,
            ended: false,
            video_ended: false,
            width,
            height,
            video_codec: report.video_codec,
            audio_codec: report.audio_codec,
            mime_type,
            encoded_bytes: report.encoded_bytes,
            frames_submitted,
            dropped_frames: 0,
            origin_clean,
        });
        self.media_failure = None;
        self.dispatch_media_state(0, "loaded")?;
        Ok(())
    }

    pub(in crate::renderer_process::child::document) fn advance_media(
        &mut self,
        _elapsed: std::time::Duration,
        connection: &mut ChildConnection,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<bool, String> {
        if connection.media_operation_pending() {
            return Ok(false);
        }
        let Some(playback) = self.media.as_mut() else {
            return Ok(false);
        };
        if !playback.playing || playback.ended {
            return Ok(false);
        }
        let snapshot = connection.video_snapshot(
            self.id,
            self.revision,
            playback.node.to_wire(),
            playback.source_id,
            playback.frame_end_100ns,
        );
        let snapshot = match snapshot {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return Ok(false),
            Err(error) => {
                self.fail_media_playback(error, connection, outcome)?;
                return Ok(false);
            }
        };
        playback.clock_100ns = snapshot.state.position_100ns;
        playback.duration_100ns = snapshot.state.duration_100ns;
        playback.playing = snapshot.state.playing;
        playback.ended = snapshot.state.ended;
        // These are producer counts; the browser can coalesce superseded frames under load.
        playback.frames_submitted = snapshot
            .published
            .saturating_add(u64::from(playback.width != 0));
        playback.dropped_frames = snapshot.dropped;
        let mut resized = false;
        if let Some(frame) = snapshot.frame {
            let metadata = frame.metadata;
            resized = playback.width != metadata.width || playback.height != metadata.height;
            playback.width = metadata.width;
            playback.height = metadata.height;
            playback.frame_end_100ns = frame_end(metadata);
            let image = DecodedImage {
                width: metadata.width,
                height: metadata.height,
                bgra: frame.bgra,
            };
            let key = self
                .page
                .install_media_frame(playback.node, image.clone())?;
            if let Some(runtime) = self.script_runtime.as_mut() {
                runtime.set_media_image(playback.node, image, playback.origin_clean);
            }
            self.sent_images.remove(&key);
        }
        let ended = playback.ended;
        let event = self.media_state_outcome(0, if ended { "ended" } else { "time" })?;
        super::super::merge_outcome(outcome, event, self.page.dom.document.id());
        // Pixel changes alone must not serialize/reinstall the entire document display list.
        Ok(resized)
    }
}
