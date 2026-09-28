//! Browser-side BroadcastChannel authority and bounded renderer delivery.

use super::tabs::TabId;
use super::*;
use better_web_browser::renderer_protocol::BroadcastCommand;

impl BrowserState {
    pub(super) fn apply_broadcast_command(
        &mut self,
        command: BroadcastCommand,
    ) -> Result<bool, String> {
        if !self.navigation.owns_document(command.document) {
            return Ok(true);
        }
        // Top-level documents own their top-level site. The browser derives this
        // origin from its committed URL rather than any renderer-supplied value.
        let document_url = self
            .navigation
            .submitted_document_url(command.document)
            .unwrap_or(&self.reader_url);
        let origin = better_web_browser::storage::storage_origin(document_url)
            .map_err(|error| format!("BroadcastChannel document origin: {error}"))?;
        self.app
            .broadcast_channels
            .borrow_mut()
            .apply(self.tabs.active_id().get(), &origin, &command)
            .map_err(str::to_owned)
    }

    pub(super) fn pump_broadcast_deliveries(&mut self, id: TabId) -> Result<(), String> {
        let Some(session) = self
            .tabs
            .get_mut(id)
            .and_then(|tab| tab.renderer_session.as_ref())
        else {
            return Ok(());
        };
        for _ in 0..64 {
            let mut result = Ok(false);
            let taken = self
                .app
                .broadcast_channels
                .borrow_mut()
                .take_if(id.get(), |delivery| {
                    result = session.try_send_broadcast_delivery(delivery.clone());
                    matches!(result, Ok(true))
                });
            result?;
            if !taken {
                break;
            }
        }
        Ok(())
    }
}
