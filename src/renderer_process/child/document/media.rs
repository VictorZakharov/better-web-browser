//! Worker-clocked video presentation and decoded-frame state.

use super::DocumentRuntime;
use crate::engine::DecodedImage;
use crate::engine::dom::NodeId;
use crate::media_process::RendererMediaDecode;
use crate::renderer_process::child::connection::ChildConnection;
const MAX_FRAMES_PER_TICK: usize = 4;
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
}

impl DocumentRuntime {
    pub(super) fn install_media_decode(
        &mut self,
        node: NodeId,
        decode: RendererMediaDecode,
    ) -> Result<(), String> {
        let metadata = decode.frame.metadata;
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
        });
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
        let actions = std::mem::take(&mut outcome.media_actions);
        for action in actions {
            let disposition = match self.media.as_mut() {
                Some(playback) if playback.node == action.node && !playback.ended => {
                    let volume = if action.muted {
                        0
                    } else {
                        action.volume_millis
                    };
                    let state = connection
                        .media()
                        .ok_or_else(|| "contained media worker is unavailable".to_string())?
                        .set_playback(playback.source_id, action.play, volume)?;
                    playback.clock_100ns = state.position_100ns;
                    playback.duration_100ns = state.duration_100ns;
                    playback.playing = state.playing;
                    playback.ended = state.ended;
                    if action.play && state.playing {
                        "playing"
                    } else if !action.play {
                        "paused"
                    } else {
                        "denied"
                    }
                }
                _ => "denied",
            };
            let target = self
                .page
                .dom
                .find_node(action.node)
                .ok_or_else(|| "media action targeted a retired node".to_string())?;
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
        Ok(())
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
            &mut self.pending_media_outcome,
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
}

fn frame_end(metadata: crate::media_protocol::MediaVideoFrameMetadata) -> u64 {
    (metadata.timestamp_100ns.max(0) as u64).saturating_add(metadata.duration_100ns)
}
