//! Media commands and their contained-worker acknowledgements.
use super::*;

impl DocumentRuntime {
    pub(super) fn apply_media_action(
        &mut self,
        action: &ScriptMediaAction,
        connection: &mut ChildConnection,
    ) -> Result<Option<&'static str>, String> {
        if let ScriptMediaCommand::SetPlayback {
            playing: true,
            volume_millis,
        } = &action.command
            && !self.media_activation.allows(*volume_millis)
        {
            return Ok(Some("not-allowed"));
        }
        if matches!(&action.command, ScriptMediaCommand::Reset) {
            connection.retire_video();
            if self
                .media
                .as_ref()
                .is_some_and(|media| media.node == action.node)
            {
                self.media.take();
            }
            self.media_failure = None;
            return Ok(Some("reset"));
        }
        if let ScriptMediaCommand::Commit { mime_type, bytes } = &action.command {
            if !supports_media_track(mime_type, "video/mp4", "avc1.")
                || !mime_type.to_ascii_lowercase().contains("mp4a.40.2")
            {
                let failure = format!("unsupported MediaSource type: {mime_type}");
                self.record_media_failure(failure.clone());
                self.pending_async_outcome.diagnostics.push(failure);
                return Ok(Some("media-error"));
            }
            return match connection.decode_media(bytes).and_then(|decode| {
                self.install_media_decode(action.node, decode, mime_type.clone())
            }) {
                Ok(()) => Ok(Some("committed")),
                Err(error) => {
                    self.record_media_failure(error.clone());
                    self.pending_async_outcome
                        .diagnostics
                        .push(format!("MediaSource decode rejected: {error}"));
                    Ok(Some("media-error"))
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
                return Ok(Some("media-error"));
            }
            let mime_type = format!("{video_mime_type} + {audio_mime_type}");
            return match connection.start_decode_media_tracks(video_bytes, audio_bytes) {
                Ok(()) => {
                    self.pending_media_action = Some(PendingMediaAction::Decode {
                        node: action.node,
                        request_id: action.request_id,
                        mime_type,
                    });
                    Ok(None)
                }
                Err(error) => {
                    self.record_media_failure(error.clone());
                    self.pending_async_outcome
                        .diagnostics
                        .push(format!("adaptive MediaSource decode rejected: {error}"));
                    Ok(Some("media-error"))
                }
            };
        }
        if let ScriptMediaCommand::AppendAdaptive {
            video_bytes,
            audio_bytes,
        } = &action.command
        {
            let Some(source_id) = self
                .media
                .as_ref()
                .filter(|playback| playback.node == action.node)
                .map(|playback| playback.source_id)
            else {
                return Ok(Some("denied"));
            };
            return match connection.start_append_media_tracks(source_id, video_bytes, audio_bytes) {
                Ok(()) => {
                    self.pending_media_action = Some(PendingMediaAction::Append {
                        node: action.node,
                        request_id: action.request_id,
                    });
                    Ok(None)
                }
                Err(error) => {
                    self.record_media_failure(error.clone());
                    self.pending_async_outcome
                        .diagnostics
                        .push(format!("adaptive MediaSource append rejected: {error}"));
                    Ok(Some("media-error"))
                }
            };
        }
        let Some(playback) = self
            .media
            .as_ref()
            .filter(|playback| playback.node == action.node)
        else {
            return Ok(Some("denied"));
        };
        let source_id = playback.source_id;
        match &action.command {
            ScriptMediaCommand::SetPlayback {
                playing,
                volume_millis,
            } => {
                let state = connection.set_media_playback(source_id, *playing, *volume_millis)?;
                self.apply_playback_state(state);
                Ok(Some(if *playing && state.playing {
                    "playing"
                } else if !*playing {
                    "paused"
                } else {
                    "denied"
                }))
            }
            ScriptMediaCommand::Configure { volume_millis } => {
                if playback.playing && !self.media_activation.allows(*volume_millis) {
                    let state = connection.set_media_playback(source_id, false, 0)?;
                    self.apply_playback_state(state);
                    return Ok(Some("paused"));
                }
                let state =
                    connection.set_media_playback(source_id, playback.playing, *volume_millis)?;
                self.apply_playback_state(state);
                Ok(Some("configured"))
            }
            ScriptMediaCommand::Seek { position_100ns } => {
                let state = connection.seek_media_playback(source_id, *position_100ns)?;
                let frame = connection.next_media_frame(source_id)?;
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
                        playback.frames_submitted = playback.frames_submitted.saturating_add(1);
                    }
                } else if let Some(playback) = self.media.as_mut() {
                    playback.video_ended = true;
                }
                Ok(Some("seeked"))
            }
            ScriptMediaCommand::Reset => unreachable!(),
            ScriptMediaCommand::Commit { .. } => unreachable!(),
            ScriptMediaCommand::CommitAdaptive { .. } => unreachable!(),
            ScriptMediaCommand::AppendAdaptive { .. } => unreachable!(),
        }
    }
}
