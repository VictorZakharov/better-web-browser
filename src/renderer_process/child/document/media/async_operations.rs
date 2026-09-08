use super::*;
use crate::renderer_process::child::connection::MediaOperationCompletion;

pub(in crate::renderer_process::child::document) enum PendingMediaAction {
    Decode {
        node: NodeId,
        request_id: u64,
        mime_type: String,
    },
    Append {
        node: NodeId,
        request_id: u64,
    },
}

impl PendingMediaAction {
    fn node(&self) -> NodeId {
        match self {
            Self::Decode { node, .. } | Self::Append { node, .. } => *node,
        }
    }

    fn request_id(&self) -> u64 {
        match self {
            Self::Decode { request_id, .. } | Self::Append { request_id, .. } => *request_id,
        }
    }
}

impl DocumentRuntime {
    pub(super) fn poll_pending_media_action(
        &mut self,
        outcome: &mut crate::engine::ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        let Some(completion) = connection.poll_media_operation()? else {
            return Ok(());
        };
        let Some(pending) = self.pending_media_action.take() else {
            if discard_replaced_media(&mut self.media, &completion) {
                connection.clear_video();
            }
            self.pending_async_outcome
                .diagnostics
                .push("discarded a retired asynchronous media completion".into());
            return Ok(());
        };
        let node = pending.node();
        let request_id = pending.request_id();
        if self.page.dom.find_node(node).is_none() {
            if discard_replaced_media(&mut self.media, &completion) {
                connection.clear_video();
            }
            return Ok(());
        }
        let disposition = match (pending, completion) {
            (
                PendingMediaAction::Decode { mime_type, .. },
                MediaOperationCompletion::Decoded(result),
            ) => match result.and_then(|decode| self.install_media_decode(node, decode, mime_type))
            {
                Ok(()) => "committed",
                Err(error) => {
                    self.record_media_failure(error.clone());
                    self.pending_async_outcome
                        .diagnostics
                        .push(format!("adaptive MediaSource decode rejected: {error}"));
                    "media-error"
                }
            },
            (PendingMediaAction::Append { .. }, MediaOperationCompletion::Appended(result)) => {
                match result {
                    Ok((encoded_bytes, duration_100ns, buffered)) => {
                        if let Some(playback) = self.media.as_mut() {
                            playback.encoded_bytes = encoded_bytes;
                            playback.buffered = buffered;
                            playback.duration_100ns = playback.duration_100ns.max(duration_100ns);
                            playback.video_ended = false;
                            playback.ended = false;
                        }
                        "appended"
                    }
                    Err(error) => {
                        self.record_media_failure(error.clone());
                        self.pending_async_outcome
                            .diagnostics
                            .push(format!("adaptive MediaSource append rejected: {error}"));
                        "media-error"
                    }
                }
            }
            _ => return Err("contained media operation completed with the wrong response".into()),
        };
        self.dispatch_media_response(outcome, node, request_id, disposition)
    }

    pub(super) fn dispatch_media_response(
        &mut self,
        outcome: &mut crate::engine::ScriptOutcome,
        node: NodeId,
        request_id: u64,
        disposition: &'static str,
    ) -> Result<(), String> {
        let target = self
            .page
            .dom
            .find_node(node)
            .ok_or_else(|| "media action target retired during dispatch".to_string())?;
        let (current_time, duration, width, height) = self.media_values(node);
        let response = self.dispatch_user_input(crate::engine::UserInputEvent::Media {
            target,
            request_id,
            disposition,
            current_time,
            duration,
            width,
            height,
            buffered: self
                .media
                .as_ref()
                .filter(|media| media.node == node)
                .map(|media| media.buffered.seconds()),
        })?;
        super::super::merge_outcome(outcome, response.outcome, self.page.dom.document.id());
        Ok(())
    }
}

fn discard_replaced_media(
    playback: &mut Option<MediaPlayback>,
    completion: &MediaOperationCompletion,
) -> bool {
    // A successful decode replaces the worker source even when its DOM owner was removed
    // before completion. Its new source starts paused. Forget the old document-side source
    // without sending it a pause: it no longer exists in the worker. Failed decodes and
    // appends do not replace source identity, so their prior playback state remains valid.
    if matches!(completion, MediaOperationCompletion::Decoded(Ok(_))) {
        playback.take();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests;
