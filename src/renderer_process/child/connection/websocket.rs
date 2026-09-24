//! Document-scoped socket IPC and guarded delivery into the retained realm.

use super::*;
use crate::renderer_protocol::{WebSocketCommand, WebSocketEvent};
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_websocket_command(
        &mut self,
        command: WebSocketCommand,
    ) -> Result<(), String> {
        command.validate().map_err(|error| error.to_string())?;
        self.writer
            .send_renderer(&RendererMessage::WebSocketCommand(command))
            .map_err(|error| error.to_string())
    }

    pub(super) fn deliver_websocket_event(&mut self, event: WebSocketEvent) -> Result<(), String> {
        event.validate().map_err(|error| error.to_string())?;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != event.document {
            self.document = Some(runtime);
            return Ok(());
        }
        let document = event.document;
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_websocket_event(event, self)
        }));
        match result {
            Ok(Ok(Some(update))) => self.send_document_update(update)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => return self.send_document_failure(document, error),
            Err(payload) => {
                return self.send_document_failure(document, super::runtime::panic_detail(payload));
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }
}
