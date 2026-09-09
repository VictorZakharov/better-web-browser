//! A media pipeline failure must not terminate the owning document's event loop.
use super::*;

impl DocumentRuntime {
    pub(super) fn fail_media_playback(
        &mut self,
        error: String,
        connection: &mut ChildConnection,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        connection.retire_video();
        let Some(playback) = self.media.as_mut() else {
            return Ok(());
        };
        playback.playing = false;
        let node = playback.node;
        self.record_media_failure(error.clone());
        outcome
            .diagnostics
            .push(format!("media playback failed: {error}"));
        self.dispatch_media_response(outcome, node, 0, "media-error")
    }
}
