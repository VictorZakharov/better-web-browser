//! Outgoing browser-owned mutation intents and completed storage-event receipts.
use super::*;
use crate::renderer_protocol::{
    CookieMutation, StateSnapshotApplied, StateSnapshotKind, StorageMutationRequest,
};
use crate::storage::StorageAreaKind;

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_state_mutations(
        &mut self,
        document: DocumentId,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        for (area, version) in outcome.storage_event_receipts.drain(..) {
            self.send_state_snapshot_applied(StateSnapshotApplied {
                document,
                version,
                kind: match area {
                    StorageAreaKind::Local => StateSnapshotKind::LocalStorage,
                    StorageAreaKind::Session => StateSnapshotKind::SessionStorage,
                },
            })?;
        }
        for action in outcome.fullscreen_actions.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::FullscreenRequest(
                    crate::renderer_protocol::FullscreenRequest {
                        document,
                        request_id: action.request_id,
                        action: if action.enter {
                            crate::renderer_protocol::FullscreenAction::Enter
                        } else {
                            crate::renderer_protocol::FullscreenAction::Exit
                        },
                    },
                ))
                .map_err(|error| error.to_string())?;
        }
        for assignment in outcome.cookie_updates.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::CookieMutation(CookieMutation {
                    document,
                    assignment,
                }))
                .map_err(|error| error.to_string())?;
        }
        for write in outcome.storage_updates.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::StorageMutation(StorageMutationRequest {
                    document,
                    sequence: write.sequence,
                    source_url: write.source_url,
                    mutation: write.mutation,
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
