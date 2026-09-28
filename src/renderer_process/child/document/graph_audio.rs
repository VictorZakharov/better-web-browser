//! Document-owned admission of Web Audio PCM into the contained output worker.

use super::*;
use crate::engine::script::ScriptGraphAudioAction;

const MAX_GRAPH_AUDIO_ACTIONS_PER_TICK: usize = 16;

impl DocumentRuntime {
    pub(super) fn apply_graph_audio_actions(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        let mut processed = 0;
        loop {
            self.poll_graph_audio_close(outcome, connection);
            let actions = std::mem::take(&mut outcome.graph_audio_actions);
            processed += actions.len();
            if processed > MAX_GRAPH_AUDIO_ACTIONS_PER_TICK {
                return Err("document exceeded the bounded Web Audio action budget".into());
            }

            // A close has priority over a chunk waiting for worker backpressure. It must
            // release the voice and settle close() even if the output queue is saturated.
            let (closes, queues): (Vec<_>, Vec<_>) = actions
                .into_iter()
                .partition(|action| matches!(action, ScriptGraphAudioAction::Close { .. }));
            let mut closed = HashSet::new();
            for action in closes {
                let ScriptGraphAudioAction::Close { stream_id } = action else {
                    unreachable!();
                };
                closed.insert(stream_id);
                if self.pending_graph_audio_chunk.as_ref().is_some_and(|pending| {
                    matches!(pending, ScriptGraphAudioAction::Queue { stream_id: id, .. } if *id == stream_id)
                }) {
                    self.pending_graph_audio_chunk = None;
                }
                if self.pending_graph_audio_start == Some(stream_id) {
                    self.pending_graph_audio_start = None;
                }
                if self
                    .pending_graph_audio_close
                    .is_none_or(|(id, _)| id != stream_id)
                {
                    if self.graph_audio_stream == Some(stream_id) {
                        self.pending_graph_audio_close = connection
                            .close_graph_pcm(self.id, u64::from(stream_id))
                            .map(|ticket| (stream_id, ticket));
                        self.graph_audio_stream = None;
                    }
                    if self.pending_graph_audio_close.is_none() {
                        self.deliver_graph_audio_status(outcome, stream_id, "closed");
                    }
                }
            }
            for action in queues {
                let ScriptGraphAudioAction::Queue { stream_id, .. } = &action else {
                    unreachable!();
                };
                let stream_id = *stream_id;
                if closed.contains(&stream_id)
                    || !self.media_activation.allows(1_000)
                    || self.pending_graph_audio_close.is_some()
                    || self.pending_graph_audio_chunk.is_some()
                    || self.pending_graph_audio_start.is_some()
                    || self.graph_audio_stream.is_some_and(|id| id != stream_id)
                    || (self.graph_audio_stream.is_none()
                        && stream_id <= self.graph_audio_last_stream_id)
                {
                    self.deliver_graph_audio_status(outcome, stream_id, "rejected");
                    continue;
                }
                self.pending_graph_audio_chunk = Some(action);
            }
            self.retry_graph_audio_chunk(outcome, connection);
            self.poll_graph_audio_start(outcome, connection);
            self.poll_graph_audio_close(outcome, connection);
            if outcome.graph_audio_actions.is_empty() {
                break;
            }
        }
        Ok(())
    }

    fn retry_graph_audio_chunk(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
    ) {
        let Some(ScriptGraphAudioAction::Queue {
            stream_id,
            format,
            pcm,
        }) = self.pending_graph_audio_chunk.as_ref()
        else {
            return;
        };
        let stream_id = *stream_id;
        let result =
            connection.try_queue_graph_pcm(self.id, u64::from(stream_id), *format, pcm.clone());
        match result {
            Ok(true) => {
                let first_chunk = self.graph_audio_stream.is_none();
                self.graph_audio_stream = Some(stream_id);
                self.graph_audio_last_stream_id = stream_id;
                self.pending_graph_audio_chunk = None;
                if first_chunk {
                    // resume() waits for the media worker to allocate a real output voice.
                    self.pending_graph_audio_start = Some(stream_id);
                } else {
                    self.deliver_graph_audio_status(outcome, stream_id, "accepted");
                }
            }
            Ok(false) => {} // Retry the same bytes on the next 10 ms document clock tick.
            Err(error) => self.reject_graph_audio(outcome, connection, stream_id, error),
        }
    }

    fn poll_graph_audio_start(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
    ) {
        let Some(stream_id) = self.pending_graph_audio_start else {
            return;
        };
        match connection.graph_pcm_first_chunk_status(self.id, u64::from(stream_id)) {
            Ok(Some(())) => {
                self.pending_graph_audio_start = None;
                self.deliver_graph_audio_status(outcome, stream_id, "accepted");
            }
            Ok(None) => {} // The worker has not acknowledged output allocation yet.
            Err(error) => self.reject_graph_audio(outcome, connection, stream_id, error),
        }
    }

    fn reject_graph_audio(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
        stream_id: u32,
        error: String,
    ) {
        outcome
            .diagnostics
            .push(format!("Web Audio output failed: {error}"));
        self.pending_graph_audio_chunk = None;
        self.pending_graph_audio_start = None;
        if self.graph_audio_stream == Some(stream_id) {
            self.pending_graph_audio_close = connection
                .close_graph_pcm(self.id, u64::from(stream_id))
                .map(|ticket| (stream_id, ticket));
            self.graph_audio_stream = None;
        }
        self.deliver_graph_audio_status(outcome, stream_id, "rejected");
    }

    fn poll_graph_audio_close(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &ChildConnection,
    ) {
        let Some((stream_id, ticket)) = self.pending_graph_audio_close else {
            return;
        };
        if connection.graph_pcm_close_completed(self.id, ticket) {
            self.pending_graph_audio_close = None;
            self.deliver_graph_audio_status(outcome, stream_id, "closed");
        }
    }

    fn deliver_graph_audio_status(
        &mut self,
        outcome: &mut ScriptOutcome,
        stream_id: u32,
        status: &str,
    ) {
        if let Some(runtime) = self.script_runtime.as_mut() {
            let delivered = runtime.deliver_graph_audio_status(stream_id, status);
            merge_outcome(outcome, delivered, self.page.dom.document.id());
        }
    }
}
