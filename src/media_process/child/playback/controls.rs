//! Control requests use the audio output clock when present and the video clock otherwise.

use super::*;
use crate::media_protocol::MediaPlaybackState;

impl Playback {
    pub(in crate::media_process::child) fn set_playback(
        &mut self,
        source_id: u64,
        playing: bool,
        volume_millis: u16,
    ) -> Result<MediaPlaybackState, String> {
        if let Some((active_source_id, audio)) = self.audio.as_ref() {
            if source_id != *active_source_id {
                return Err(format!(
                    "stale playback control for source {source_id}; expected {active_source_id}"
                ));
            }
            return audio.set_playback(playing, volume_millis);
        }
        let Some(clock) = self
            .video_clock
            .as_mut()
            .filter(|clock| clock.source_id() == source_id)
        else {
            return Err("media worker received playback control with no active source".into());
        };
        Ok(clock.set_playback(playing))
    }

    pub(in crate::media_process::child) fn playback_state(
        &self,
        source_id: u64,
    ) -> Result<MediaPlaybackState, String> {
        if let Some((active_source_id, audio)) = self.audio.as_ref() {
            if source_id != *active_source_id {
                return Err(format!(
                    "stale playback query for source {source_id}; expected {active_source_id}"
                ));
            }
            return audio.state();
        }
        self.video_clock
            .as_ref()
            .filter(|clock| clock.source_id() == source_id)
            .map(VideoClock::state)
            .ok_or_else(|| "media worker received playback query with no active source".into())
    }

    pub(in crate::media_process::child) fn seek(
        &mut self,
        source_id: u64,
        position_100ns: u64,
    ) -> Result<MediaPlaybackState, String> {
        if self.pending.is_some() {
            return Err("media worker received a seek before acknowledging its frame".into());
        }
        if let Some((active_source_id, video)) = self.active.as_mut() {
            if source_id != *active_source_id {
                return Err(format!(
                    "stale playback seek for source {source_id}; expected {active_source_id}"
                ));
            }
            video.seek(position_100ns)?;
        }
        if let Some((audio_source_id, audio)) = self.audio.as_ref() {
            if source_id != *audio_source_id {
                return Err("media worker audio/video source identity disagreed".into());
            }
            return audio.seek(position_100ns);
        }
        self.video_clock
            .as_mut()
            .filter(|clock| clock.source_id() == source_id)
            .map(|clock| clock.seek(position_100ns))
            .ok_or_else(|| "media worker received a seek with no active clock".into())
    }
}
