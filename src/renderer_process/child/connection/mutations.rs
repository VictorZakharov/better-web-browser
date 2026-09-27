//! Outgoing browser-owned mutation intents and completed storage-event receipts.
use super::*;
use crate::renderer_protocol::{
    CookieMutation, PolicyMutation, StateSnapshotApplied, StateSnapshotKind, StorageMutationRequest,
};
use crate::storage::StorageAreaKind;

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_policy_updates(
        &mut self,
        document: DocumentId,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        for update in outcome.policy_updates.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::PolicyMutation(PolicyMutation {
                    document,
                    client_id: update.client.id,
                    serialized: update.serialized,
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn send_state_mutations(
        &mut self,
        document: DocumentId,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        self.send_policy_updates(document, outcome)?;
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
        for action in outcome.pointer_lock_actions.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::PointerLockRequest(
                    crate::renderer_protocol::PointerLockRequest {
                        document,
                        request_id: action.request_id,
                        target: action.target.and_then(|node| {
                            crate::renderer_protocol::DocumentNodeId::new(node.to_wire()).ok()
                        }),
                    },
                ))
                .map_err(|error| error.to_string())?;
        }
        self.send_cookie_updates(document, outcome)?;
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

    /// Flush state that the browser must apply before any request started in the
    /// same script task. Leave navigation, storage, and UI commands in their
    /// normal state-mutation checkpoint.
    pub(in crate::renderer_process::child) fn send_network_state_updates(
        &mut self,
        document: DocumentId,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        self.send_policy_updates(document, outcome)?;
        self.send_cookie_updates(document, outcome)
    }

    fn send_cookie_updates(
        &mut self,
        document: DocumentId,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        for assignment in outcome.cookie_updates.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::CookieMutation(CookieMutation {
                    document,
                    assignment,
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
