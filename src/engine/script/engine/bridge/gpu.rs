//! Resource uploads have registry/lifetime semantics, unlike stateless painters.
use super::*;

impl HostBridge {
    pub(super) fn dispatch_owned_gpu(&self, arguments: &mut [JsValue]) -> JsResult<JsValue> {
        let operation = arguments
            .first()
            .map(JsValue::string_value)
            .unwrap_or_default();
        let run = if operation == "webglReadPixels" {
            crate::engine::script::canvas_host::webgl::read_pixels_owned
        } else {
            crate::engine::script::canvas_host::webgl::command_owned
        };
        match self {
            Self::Document(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                let started = host.host_call_profile.start();
                let result = run(arguments, &mut host.webgl);
                host.host_call_profile.record(&operation, started);
                // The command string remains intact after its independent byte
                // copy is consumed. Preserve the fixed-label attribution path.
                if operation == "webglCommand" {
                    host.host_call_profile
                        .record_webgl_command(arguments, started);
                }
                result
            }
            Self::Worker(host) => {
                let host = host.upgrade().ok_or_else(inactive_host)?;
                let mut host = host.borrow_mut();
                run(arguments, &mut host.webgl)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_gpu_does_not_bypass_document_or_worker_host_liveness() {
        for bridge in [
            HostBridge::Document(Weak::new()),
            HostBridge::Worker(Weak::new()),
        ] {
            for operation in ["webglCommand", "webglReadPixels"] {
                let mut args = [
                    JsValue::from(operation.to_owned()),
                    JsValue::from(1),
                    JsValue::from("{\"op\":\"bufferData\"}".to_owned()),
                    JsValue::Bytes(vec![19; 4]),
                ];
                let pointer = args[3].as_bytes().unwrap().as_ptr();
                let error = bridge.dispatch_owned(&mut args).unwrap_err();
                assert!(error.message.contains("not active"));
                assert_eq!(args[3].as_bytes().unwrap().as_ptr(), pointer);
            }
        }
    }
}
