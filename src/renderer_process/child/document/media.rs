//! Worker-clocked video presentation and decoded-frame state.

mod actions;
mod async_operations;
mod frame_presentation;
mod policy;
pub(super) use policy::MediaActivation;

use super::DocumentRuntime;
use crate::engine::DecodedImage;
use crate::engine::dom::NodeId;
use crate::engine::script::{ScriptMediaAction, ScriptMediaCommand};
use crate::media_process::RendererMediaDecode;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::MediaRuntimeReport;
pub(super) use async_operations::PendingMediaAction;
const MAX_MEDIA_ACTIONS_PER_TICK: usize = 32;
const CLOCK_POLL_MICROS: u64 = 20_000;

pub(super) struct MediaPlayback {
    node: NodeId,
    source_id: u64,
    clock_100ns: u64,
    frame_end_100ns: u64,
    duration_100ns: u64,
    buffered: crate::media_protocol::MediaBufferedExtent,
    playing: bool,
    ended: bool,
    video_ended: bool,
    width: u32,
    height: u32,
    mime_type: String,
    encoded_bytes: u64,
    frames_submitted: u64,
    dropped_frames: u64,
}

impl DocumentRuntime {
    pub(super) fn apply_media_actions(
        &mut self,
        outcome: &mut crate::engine::ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        self.poll_pending_media_action(outcome, connection)?;
        // A navigation can replace the document while its media operation completes.
        // Defer the new document's commands rather than failing the shared worker.
        if self.pending_media_action.is_some() || connection.media_operation_pending() {
            self.pending_async_outcome
                .media_actions
                .append(&mut outcome.media_actions);
            return Ok(());
        }
        let mut processed = 0_usize;
        while !outcome.media_actions.is_empty() {
            let actions = std::mem::take(&mut outcome.media_actions);
            processed = processed
                .checked_add(actions.len())
                .ok_or_else(|| "media action count overflow".to_string())?;
            if processed > MAX_MEDIA_ACTIONS_PER_TICK {
                return Err("document exceeded the bounded media action budget".into());
            }
            for action in actions {
                if self.pending_media_action.is_some() {
                    self.pending_async_outcome.media_actions.push(action);
                    continue;
                }
                if self.page.dom.find_node(action.node).is_none() {
                    self.stop_retired_media(action.node, connection)?;
                    continue;
                }
                if let Some(disposition) = self.apply_media_action(&action, connection)? {
                    self.dispatch_media_response(
                        outcome,
                        action.node,
                        action.request_id,
                        disposition,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn stop_retired_media(
        &mut self,
        node: NodeId,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        let Some(playback) = self.media.as_ref().filter(|media| media.node == node) else {
            return Ok(());
        };
        let source_id = playback.source_id;
        connection.set_media_playback(source_id, false, 0)?;
        connection.clear_video();
        self.media.take();
        Ok(())
    }

    fn apply_playback_state(&mut self, state: crate::media_protocol::MediaPlaybackState) {
        if let Some(playback) = self.media.as_mut() {
            playback.clock_100ns = state.position_100ns;
            playback.duration_100ns = state.duration_100ns;
            playback.playing = state.playing;
            playback.ended = state.ended;
        }
    }

    pub(super) fn media_timer_micros(&self) -> Option<u64> {
        if self.pending_media_action.is_some() {
            return Some(CLOCK_POLL_MICROS);
        }
        let playback = self.media.as_ref()?;
        if !playback.playing || playback.ended {
            return None;
        }
        let remaining = playback
            .frame_end_100ns
            .saturating_sub(playback.clock_100ns);
        Some(remaining.saturating_div(10).clamp(1, CLOCK_POLL_MICROS))
    }

    fn dispatch_media_state(
        &mut self,
        request_id: u64,
        disposition: &'static str,
    ) -> Result<(), String> {
        let response = self.media_state_outcome(request_id, disposition)?;
        super::merge_outcome(
            &mut self.pending_async_outcome,
            response,
            self.page.dom.document.id(),
        );
        Ok(())
    }

    fn media_state_outcome(
        &mut self,
        request_id: u64,
        disposition: &'static str,
    ) -> Result<crate::engine::ScriptOutcome, String> {
        let Some(playback) = self.media.as_ref() else {
            return Ok(crate::engine::ScriptOutcome::default());
        };
        let Some(target) = self.page.dom.find_node(playback.node) else {
            return Ok(crate::engine::ScriptOutcome::default());
        };
        let (current_time, duration, width, height) = self.media_values(playback.node);
        self.dispatch_user_input(crate::engine::UserInputEvent::Media {
            target,
            request_id,
            disposition,
            current_time,
            duration,
            width,
            height,
            buffered: Some(playback.buffered.seconds()),
        })
        .map(|response| response.outcome)
    }

    fn media_values(&self, node: NodeId) -> (f64, f64, u32, u32) {
        self.media
            .as_ref()
            .filter(|playback| playback.node == node)
            .map(|playback| {
                (
                    playback.clock_100ns as f64 / 10_000_000.0,
                    playback.duration_100ns as f64 / 10_000_000.0,
                    playback.width,
                    playback.height,
                )
            })
            .unwrap_or((0.0, f64::NAN, 0, 0))
    }

    pub(super) fn record_media_failure(&mut self, detail: String) {
        self.media_failure = Some(detail);
    }

    pub(super) fn media_runtime_report(&self) -> Option<MediaRuntimeReport> {
        self.media
            .as_ref()
            .map(|playback| MediaRuntimeReport {
                active: true,
                playing: playback.playing,
                ended: playback.ended,
                current_time_100ns: playback.clock_100ns,
                duration_100ns: playback.duration_100ns,
                backend: "Windows Media Foundation / XAudio2".into(),
                mime_type: playback.mime_type.clone(),
                video_codec: "H.264".into(),
                audio_codec: "AAC-LC".into(),
                encoded_queue_bytes: playback.encoded_bytes,
                // The worker retains committed segments for seeking. The 8 MiB
                // queue limit applies to each IPC batch; this cumulative value
                // is governed by the total encoded-media limit.
                encoded_queue_limit_bytes: crate::limits::MAX_MEDIA_ENCODED_BYTES as u64,
                // Frames cross the contained boundary one at a time and are acknowledged before
                // the renderer accepts another, so a completed runtime snapshot has no outstanding
                // decoded-frame queue even while one presented image is retained for compositing.
                decoded_frame_queue_depth: 0,
                decoded_frame_queue_limit: 1,
                frames_submitted: playback.frames_submitted,
                dropped_frames: playback.dropped_frames,
                width: playback.width,
                height: playback.height,
                failure: self.media_failure.clone(),
            })
            .or_else(|| {
                self.media_failure
                    .as_ref()
                    .map(|failure| MediaRuntimeReport {
                        failure: Some(failure.clone()),
                        ..MediaRuntimeReport::default()
                    })
            })
    }
}

fn frame_end(metadata: crate::media_protocol::MediaVideoFrameMetadata) -> u64 {
    (metadata.timestamp_100ns.max(0) as u64).saturating_add(metadata.duration_100ns)
}

fn supports_media_track(mime_type: &str, essence: &str, codec: &str) -> bool {
    let mime_type = mime_type.to_ascii_lowercase();
    mime_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim() == essence)
        && mime_type.contains(codec)
}
