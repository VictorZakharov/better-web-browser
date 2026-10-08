//! Stateless painting can consume native byte copies without touching author
//! ArrayBuffers. Other host calls retain their established borrowed contract.
use super::*;

impl HostBridge {
    pub(super) fn dispatch_owned(&self, arguments: &mut [JsValue]) -> JsResult<JsValue> {
        if arguments.first().map(JsValue::string_value).as_deref() != Some("canvasPaintSolidPath") {
            return self.dispatch(arguments);
        }
        // This operation has no DOM, resource registry or policy side effects.
        // Preserve host liveness and the same native/bridge profile boundary.
        match self {
            Self::Document(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                let started = host.host_call_profile.start();
                let result = crate::engine::script::canvas_host::owned_solid_path(arguments);
                host.host_call_profile
                    .record("canvasPaintSolidPath", started);
                Ok(result)
            }
            Self::Worker(host) => {
                let _host = host.upgrade().ok_or_else(inactive_host)?;
                Ok(crate::engine::script::canvas_host::owned_solid_path(
                    arguments,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_paint_cannot_bypass_document_or_worker_lifetime_checks() {
        for bridge in [
            HostBridge::Document(Weak::new()),
            HostBridge::Worker(Weak::new()),
        ] {
            let error = bridge
                .dispatch_owned(&mut [JsValue::from("canvasPaintSolidPath".to_owned())])
                .unwrap_err();
            assert!(error.message.contains("not active"));
        }
    }
}
