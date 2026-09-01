//! Worker-clocked video presentation and decoded-frame state.

use super::DocumentRuntime;
use crate::engine::DecodedImage;
use crate::engine::dom::NodeId;
use crate::engine::script::{ScriptMediaAction, ScriptMediaCommand};
use crate::media_process::RendererMediaDecode;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::MediaRuntimeReport;
const MAX_FRAMES_PER_TICK: usize = 4;
const MAX_MEDIA_ACTIONS_PER_TICK: usize = 32;
const CLOCK_POLL_MICROS: u64 = 20_000;

pub(super) struct MediaPlayback {
    node: NodeId,
    source_id: u64,
    clock_100ns: u64,
    frame_end_100ns: u64,
    duration_100ns: u64,
    playing: bool,
    ended: bool,
    video_ended: bool,
    width: u32,
    height: u32,
    mime_type: String,
    encoded_bytes: u64,
    frames_presented: u64,
    dropped_frames: u64,
}

impl DocumentRuntime {
    pub(super) fn install_media_decode(
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
            duration_100ns: decode.report.duration_100ns,
            playing: false,
            ended: false,
            video_ended: false,
            width: metadata.width,
            height: metadata.height,
            mime_type,
            encoded_bytes: report.encoded_bytes,
            frames_presented: 1,
            dropped_frames: 0,
        });
        self.media_failure = None;
        self.dispatch_media_state(0, "loaded")?;
        Ok(())
    }

    pub(super) fn advance_media(
        &mut self,
        _elapsed: std::time::Duration,
        connection: &mut ChildConnection,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<bool, String> {
        let Some(playback) = self.media.as_mut() else {
            return Ok(false);
        };
        if !playback.playing || playback.ended {
            return Ok(false);
        }
        let source_id = playback.source_id;
        let state = connection
            .media()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?
            .playback_state(source_id)?;
        playback.clock_100ns = state.position_100ns;
        playback.duration_100ns = state.duration_100ns;
        playback.playing = state.playing;
        playback.ended = state.ended;
        if state.ended {
            let event = self.media_state_outcome(0, "ended")?;
            super::merge_outcome(outcome, event, self.page.dom.document.id());
            return Ok(true);
        }
        let mut changed = false;
        for _ in 0..MAX_FRAMES_PER_TICK {
            let Some(playback) = self.media.as_ref() else {
                break;
            };
            if playback.clock_100ns < playback.frame_end_100ns
                || playback.ended
                || playback.video_ended
            {
                break;
            }
            let source_id = playback.source_id;
            let frame = connection
                .media()
                .ok_or_else(|| "contained media worker is unavailable".to_string())?
                .next_frame(source_id)?;
            let Some(frame) = frame else {
                if let Some(playback) = self.media.as_mut() {
                    playback.video_ended = true;
                }
                break;
            };
            let metadata = frame.metadata;
            let node = self.media.as_ref().unwrap().node;
            let key = self.page.install_media_frame(
                node,
                DecodedImage {
                    width: metadata.width,
                    height: metadata.height,
                    bgra: frame.bgra,
                },
            )?;
            self.sent_images.remove(&key);
            if let Some(playback) = self.media.as_mut() {
                playback.frame_end_100ns = frame_end(metadata);
                playback.width = metadata.width;
                playback.height = metadata.height;
                playback.frames_presented = playback.frames_presented.saturating_add(1);
            }
            let event = self.media_state_outcome(0, "time")?;
            super::merge_outcome(outcome, event, self.page.dom.document.id());
            changed = true;
        }
        Ok(changed)
    }

    pub(super) fn apply_media_actions(
        &mut self,
        outcome: &mut crate::engine::ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
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
                if self.page.dom.find_node(action.node).is_none() {
                    self.stop_retired_media(action.node, connection)?;
                    continue;
                }
                let disposition = self.apply_media_action(&action, connection)?;
                let target = self
                    .page
                    .dom
                    .find_node(action.node)
                    .ok_or_else(|| "media action target retired during dispatch".to_string())?;
                let (current_time, duration, width, height) = self.media_values(action.node);
                let response = self.dispatch_user_input(crate::engine::UserInputEvent::Media {
                    target,
                    request_id: action.request_id,
                    disposition,
                    current_time,
                    duration,
                    width,
                    height,
                })?;
                super::merge_outcome(outcome, response.outcome, self.page.dom.document.id());
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
        if let Some(worker) = connection.media() {
            worker.set_playback(source_id, false, 0)?;
        }
        self.media.take();
        Ok(())
    }

    fn apply_media_action(
        &mut self,
        action: &ScriptMediaAction,
        connection: &mut ChildConnection,
    ) -> Result<&'static str, String> {
        if matches!(&action.command, ScriptMediaCommand::Reset) {
            if self
                .media
                .as_ref()
                .is_some_and(|media| media.node == action.node)
            {
                self.media.take();
            }
            self.media_failure = None;
            return Ok("reset");
        }
        if let ScriptMediaCommand::Commit { mime_type, bytes } = &action.command {
            if !supports_media_track(mime_type, "video/mp4", "avc1.")
                || !mime_type.to_ascii_lowercase().contains("mp4a.40.2")
            {
                let failure = format!("unsupported MediaSource type: {mime_type}");
                self.record_media_failure(failure.clone());
                self.pending_async_outcome.diagnostics.push(failure);
                return Ok("media-error");
            }
            return match connection.decode_media(bytes).and_then(|decode| {
                self.install_media_decode(action.node, decode, mime_type.clone())
            }) {
                Ok(()) => Ok("committed"),
                Err(error) => {
                    self.record_media_failure(error.clone());
                    self.pending_async_outcome
                        .diagnostics
                        .push(format!("MediaSource decode rejected: {error}"));
                    Ok("media-error")
                }
            };
        }
        if let ScriptMediaCommand::CommitAdaptive {
            video_mime_type,
            video_bytes,
            audio_mime_type,
            audio_bytes,
        } = &action.command
        {
            if !supports_media_track(video_mime_type, "video/mp4", "avc1.")
                || !supports_media_track(audio_mime_type, "audio/mp4", "mp4a.40.2")
            {
                let failure = format!(
                    "unsupported adaptive MediaSource types: {video_mime_type} / {audio_mime_type}"
                );
                self.record_media_failure(failure.clone());
                self.pending_async_outcome.diagnostics.push(failure);
                return Ok("media-error");
            }
            let mime_type = format!("{video_mime_type} + {audio_mime_type}");
            return match connection
                .decode_media_tracks(video_bytes, audio_bytes)
                .and_then(|decode| self.install_media_decode(action.node, decode, mime_type))
            {
                Ok(()) => Ok("committed"),
                Err(error) => {
                    self.record_media_failure(error.clone());
                    self.pending_async_outcome
                        .diagnostics
                        .push(format!("adaptive MediaSource decode rejected: {error}"));
                    Ok("media-error")
                }
            };
        }
        let Some(playback) = self
            .media
            .as_ref()
            .filter(|playback| playback.node == action.node)
        else {
            return Ok("denied");
        };
        let source_id = playback.source_id;
        let worker = connection
            .media()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        match &action.command {
            ScriptMediaCommand::SetPlayback {
                playing,
                volume_millis,
            } => {
                let state = worker.set_playback(source_id, *playing, *volume_millis)?;
                self.apply_playback_state(state);
                Ok(if *playing && state.playing {
                    "playing"
                } else if !*playing {
                    "paused"
                } else {
                    "denied"
                })
            }
            ScriptMediaCommand::Configure { volume_millis } => {
                let state = worker.set_playback(source_id, playback.playing, *volume_millis)?;
                self.apply_playback_state(state);
                Ok("configured")
            }
            ScriptMediaCommand::Seek { position_100ns } => {
                let state = worker.seek_playback(source_id, *position_100ns)?;
                let frame = worker.next_frame(source_id)?;
                self.apply_playback_state(state);
                if let Some(frame) = frame {
                    let metadata = frame.metadata;
                    let key = self.page.install_media_frame(
                        action.node,
                        DecodedImage {
                            width: metadata.width,
                            height: metadata.height,
                            bgra: frame.bgra,
                        },
                    )?;
                    self.sent_images.remove(&key);
                    if let Some(playback) = self.media.as_mut() {
                        playback.frame_end_100ns = frame_end(metadata);
                        playback.video_ended = false;
                        playback.width = metadata.width;
                        playback.height = metadata.height;
                        playback.frames_presented = playback.frames_presented.saturating_add(1);
                    }
                } else if let Some(playback) = self.media.as_mut() {
                    playback.video_ended = true;
                }
                Ok("seeked")
            }
            ScriptMediaCommand::Reset => unreachable!(),
            ScriptMediaCommand::Commit { .. } => unreachable!(),
            ScriptMediaCommand::CommitAdaptive { .. } => unreachable!(),
        }
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
                encoded_queue_limit_bytes: crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES as u64,
                // Frames cross the contained boundary one at a time and are acknowledged before
                // the renderer accepts another, so a completed runtime snapshot has no outstanding
                // decoded-frame queue even while one presented image is retained for compositing.
                decoded_frame_queue_depth: 0,
                decoded_frame_queue_limit: 1,
                frames_presented: playback.frames_presented,
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
