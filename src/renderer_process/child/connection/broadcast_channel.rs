//! Document-scoped BroadcastChannel event delivery.

use super::*;
use crate::renderer_protocol::BroadcastDelivery;
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(super) fn deliver_broadcast(&mut self, delivery: BroadcastDelivery) -> Result<(), String> {
        delivery.validate().map_err(|error| error.to_string())?;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != delivery.document {
            self.document = Some(runtime);
            return Ok(());
        }
        let document = delivery.document;
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_broadcast(delivery, self)
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
