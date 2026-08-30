//! Document-owned video presentation clock and decoded-frame state.

use super::DocumentRuntime;
use crate::engine::DecodedImage;
use crate::engine::dom::NodeId;
use crate::media_process::RendererMediaDecode;
use crate::renderer_process::child::connection::ChildConnection;
use std::time::Duration;

const MAX_FRAMES_PER_TICK: usize = 4;

pub(super) struct MediaPlayback {
    node: NodeId,
    source_id: u64,
    clock_100ns: u64,
    frame_end_100ns: u64,
    duration_100ns: u64,
    playing: bool,
    ended: bool,
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
        let autoplay = self
            .page
            .dom
            .find_node(node)
            .is_some_and(|node| node.attr("autoplay").is_some());
        self.media = Some(MediaPlayback {
            node,
            source_id: metadata.source_id,
            clock_100ns: metadata.timestamp_100ns.max(0) as u64,
            frame_end_100ns: frame_end(metadata),
            duration_100ns: decode.report.duration_100ns,
            playing: autoplay,
            ended: false,
            width: metadata.width,
            height: metadata.height,
        });
        self.dispatch_media_state(0, "loaded")?;
        if autoplay {
            self.dispatch_media_state(0, "playing")?;
        }
        Ok(())
    }

    pub(super) fn advance_media(
        &mut self,
        elapsed: Duration,
        connection: &mut ChildConnection,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<bool, String> {
        let Some(playback) = self.media.as_mut() else {
            return Ok(false);
        };
        if !playback.playing || playback.ended {
            return Ok(false);
        }
        let elapsed_100ns = elapsed.as_nanos().saturating_div(100).min(u64::MAX as u128) as u64;
        playback.clock_100ns = playback
            .clock_100ns
            .saturating_add(elapsed_100ns)
            .min(playback.duration_100ns);
        let mut changed = false;
        for _ in 0..MAX_FRAMES_PER_TICK {
            let Some(playback) = self.media.as_ref() else {
                break;
            };
            if playback.clock_100ns < playback.frame_end_100ns || playback.ended {
                break;
            }
            let source_id = playback.source_id;
            let frame = connection
                .media()
                .ok_or_else(|| "contained media worker is unavailable".to_string())?
                .next_frame(source_id)?;
            let Some(frame) = frame else {
                if let Some(playback) = self.media.as_mut() {
                    playback.ended = true;
                    playback.playing = false;
                }
                let event = self.media_state_outcome(0, "ended")?;
                super::merge_outcome(outcome, event, self.page.dom.document.id());
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
    ) -> Result<(), String> {
        let actions = std::mem::take(&mut outcome.media_actions);
        for action in actions {
            let disposition = match self.media.as_mut() {
                Some(playback) if playback.node == action.node && !playback.ended => {
                    playback.playing = action.play;
                    if action.play { "playing" } else { "paused" }
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
        Some(remaining.saturating_div(10).max(1))
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
