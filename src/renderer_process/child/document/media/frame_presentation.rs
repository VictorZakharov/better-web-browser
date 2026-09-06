//! Document-side media events and snapshots; video cadence belongs to the independent producer.
use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child::document) fn install_media_decode(
        &mut self,
        node: NodeId,
        decode: RendererMediaDecode,
        mime_type: String,
    ) -> Result<(), String> {
        let metadata = decode.frame.metadata;
        let report = decode.report;
        let key = self.page.install_media_frame(
            node,
            DecodedImage {
                width: metadata.width,
                height: metadata.height,
                bgra: decode.frame.bgra,
            },
        )?;
        self.sent_images.remove(&key);
        self.media = Some(MediaPlayback {
            node,
            source_id: metadata.source_id,
            clock_100ns: metadata.timestamp_100ns.max(0) as u64,
            frame_end_100ns: frame_end(metadata),
            duration_100ns: report.duration_100ns,
            buffered: report.buffered,
            playing: false,
            ended: false,
            video_ended: false,
            width: metadata.width,
            height: metadata.height,
            mime_type,
            encoded_bytes: report.encoded_bytes,
            frames_submitted: 1,
            dropped_frames: 0,
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
        let Some(snapshot) = connection.video_snapshot(
            self.id,
            self.revision,
            playback.node.to_wire(),
            playback.source_id,
            playback.frame_end_100ns,
        )?
        else {
            return Ok(false);
        };
        playback.clock_100ns = snapshot.state.position_100ns;
        playback.duration_100ns = snapshot.state.duration_100ns;
        playback.playing = snapshot.state.playing;
        playback.ended = snapshot.state.ended;
        // These are producer counts; the browser can coalesce superseded frames under load.
        playback.frames_submitted = snapshot.published.saturating_add(1);
        playback.dropped_frames = snapshot.dropped;
        let mut resized = false;
        if let Some(frame) = snapshot.frame {
            let metadata = frame.metadata;
            resized = playback.width != metadata.width || playback.height != metadata.height;
            playback.width = metadata.width;
            playback.height = metadata.height;
            playback.frame_end_100ns = frame_end(metadata);
            let key = self.page.install_media_frame(
                playback.node,
                DecodedImage {
                    width: metadata.width,
                    height: metadata.height,
                    bgra: frame.bgra,
                },
            )?;
            self.sent_images.remove(&key);
        }
        let ended = playback.ended;
        let event = self.media_state_outcome(0, if ended { "ended" } else { "time" })?;
        super::super::merge_outcome(outcome, event, self.page.dom.document.id());
        // Pixel changes alone must not serialize/reinstall the entire document display list.
        Ok(resized)
    }
}
