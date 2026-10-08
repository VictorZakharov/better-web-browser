//! Stateless painting can consume native byte copies without touching author
//! ArrayBuffers. Stateful GPU upload routing has its own lifetime/registry boundary.
use super::*;

impl HostBridge {
    pub(super) fn dispatch_owned(&self, arguments: &mut [JsValue]) -> JsResult<JsValue> {
        let operation = arguments
            .first()
            .map(JsValue::string_value)
            .unwrap_or_default();
        #[cfg(windows)]
        if matches!(operation.as_str(), "webglCommand" | "webglReadPixels")
            && arguments.get(3).and_then(JsValue::as_bytes).is_some()
        {
            return self.dispatch_owned_gpu(arguments);
        }
        let Some(paint) = crate::engine::script::canvas_host::owned_painter(&operation) else {
            return self.dispatch(arguments);
        };
        // This operation has no DOM, resource registry or policy side effects.
        // Preserve host liveness and the same native/bridge profile boundary.
        match self {
            Self::Document(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                let started = host.host_call_profile.start();
                let result = paint(arguments);
                host.host_call_profile.record(&operation, started);
                Ok(result)
            }
            Self::Worker(host) => {
                let _host = host.upgrade().ok_or_else(inactive_host)?;
                Ok(paint(arguments))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_paint_cannot_bypass_document_or_worker_lifetime_checks() {
        for operation in [
            "canvasFilterGaussian",
            "canvasPaintSolidPath",
            "canvasPaintShadowPath",
            "canvasPaintSourceLayer",
            "canvasCompositeLayer",
            "canvasPaintImage",
            "canvasPaintRectangle",
            "canvasPaintGradientMask",
            "canvasPaintPatternMask",
            "canvasPaintGlyphs",
            "canvasPaintSolidMask",
            "canvasPaintGradientPath",
            "canvasPaintPatternPath",
        ] {
            for bridge in [
                HostBridge::Document(Weak::new()),
                HostBridge::Worker(Weak::new()),
            ] {
                let error = bridge
                    .dispatch_owned(&mut [JsValue::from(operation.to_owned())])
                    .unwrap_err();
                assert!(error.message.contains("not active"));
            }
        }
    }
}
